# Quadruple Bookkeeping — Praetor's Verification Architecture

## The Core Thesis

A single source of truth is fragile. Four mutually-reinforcing pillars create a system where the AI cannot silently introduce error because **every claim in one pillar must be corroborated by the other three.**

| Pillar | Format | Role | Source |
|--------|--------|------|--------|
| **Code** | Rust, F#, Go, C#, Python, etc. | Implementation (what the machine executes) | AI-generated, verified by compiler |
| **Docs** | Markdown (`.md`) | Intent (what the human asked for) | AI-generated from requirements, or human-authored |
| **State Graph** | JSON + Mermaid | Topology (what states exist, what transitions are legal) | AI-generated, stored in `.praetor/state-graph.json` |
| **Facts** | Datalog (`crepe`) | Invariants (the laws the code must obey) | Extracted from AST by Praetor, checked against rules in `.praetor/rules.dl` |

**Discrepancy in any pair → rejection.** The AI cannot fudge the Datalog query, cannot fake a state transition that wasn't declared, and cannot write code that contradicts its own documented intent.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Praetor (Rust binary)                     │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐   │
│  │  LSP      │  │  Report  │  │  Facts   │  │  State Graph   │   │
│  │  Server   │  │  CLI     │  │  Engine  │  │  Engine        │   │
│  │ (default) │  │ (report) │  │ (crepe)  │  │ (validation)   │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬─────────┘   │
│       │              │              │               │            │
│       └──────────────┴──────────────┴───────────────┘            │
│                              │                                    │
│                          tree-sitter                              │
│                        (AST parsing)                              │
└──────────────────────────────────────────────────────────────────┘
```

---

## Phase 7A — Datalog Fact Extractor

**Goal:** Walk every function in every file opened in the editor, extract structural facts, and feed them to a Crepe Datalog engine. Violations appear as LSP diagnostics in real time.

### New Module: `src/facts/`

| File | Purpose |
|------|---------|
| `src/facts/mod.rs` | `FactExtractor` struct — orchestrates extraction, defines Crepe relations |
| `src/facts/rules.rs` | All Crepe `rule!` macros — the invariants that code must satisfy |
| `src/facts/lsp.rs` | Convert Crepe query results → LSP `Diagnostic` |

### Core Relations (startup set)

```datalog
.input calls(Function, Callee)          // F calls Callee
.input accesses(Function, Resource)     // F reads/writes Resource
.input declares(Function, Variable)     // F declares V
.input transition(From, Event, To)      // State graph edge
.input annotated(Function, Intent)      // F has a doc comment with intent
.input parameter(Function, Param)       // F takes parameter P
```

### Built-in Rules (shipped with Praetor)

```datalog
// Rule 1: No private data access without authentication
violation("unauthenticated_access", F) :-
    accesses(F, "private_data"),
    !calls(F, "authenticate").

// Rule 2: No unreachable handler
violation("unreachable_handler", F) :-
    annotated(F, _),
    !calls(_, F),
    F != "main".

// Rule 3: All declared variables must be read
violation("unused_variable", F, V) :-
    declares(F, V),
    !accesses(F, V).

// Rule 4: No direct transition from A to C without going through B
violation("illegal_transition", F, From, To) :-
    calls(F, "transition_to"),
    parameter(F, To),
    transition(From, _, To),
    transition(From, _, "intermediate"),
    !calls(F, "check_intermediate").
```

### How It Runs

1. User opens a file in the editor
2. `didOpen`/`didChange` fires in `lsp.rs`
3. Praetor parses the AST (tree-sitter, already implemented)
4. `FactExtractor` walks the AST and emits facts into the Crepe working memory
5. Crepe evaluates all rules
6. Any `violation` facts are converted to LSP diagnostics and pushed to the client

**Incremental:** Only re-extract facts for the changed function, not the whole file. Crepe's semi-naive evaluation handles incrementality automatically.

### Dependencies

```toml
crepe = "0.2"        # Datalog in Rust as a proc macro
```

---

## Phase 7B — State Graph Module

**Goal:** Two-way validation between a declared state graph and actual code.

### New Module: `src/graph/`

| File | Purpose |
|------|---------|
| `src/graph/mod.rs` | `StateGraph` struct — load, validate, query |
| `src/graph/schema.rs` | Graph representation types |
| `src/graph/validate.rs` | Walk AST call sites, check against allowed transitions |

### Graph Format (`.praetor/state-graph.json`)

```json
{
  "states": ["Idle", "Authenticating", "Active", "Error"],
  "initial": "Idle",
  "transitions": [
    { "from": "Idle", "event": "LOGIN", "to": "Authenticating" },
    { "from": "Authenticating", "event": "SUCCESS", "to": "Active" },
    { "from": "Authenticating", "event": "FAIL", "to": "Idle" },
    { "from": "Active", "event": "LOGOUT", "to": "Idle" },
    { "from": "Active", "event": "ERROR", "to": "Error" }
  ]
}
```

### Auto-Detection from Code

For Rust and F# specifically, the module can parse `enum`-based state machines:

```rust
// tree-sitter detects this enum → generates facts:
// state("Pending"), state("Active"), state("Suspended")
// transition("Pending", "activate", "Active")
// transition("Active", "suspend", "Suspended")
enum AccountState { Pending, Active, Suspended }
```

The auto-detected graph is **compared** against the declared graph. If they differ → diagnostic.

### Validation Rules

1. Every call to a state-transition function must pass a valid target state
2. No function may read data in a state it did not authenticate to reach
3. Every declared state must be reachable from the initial state
4. No declared state may be unreachable (dead state detection)

---

## Phase 7C — LSP Enhancements

**Goal:** Make the quadruple bookkeeping visible in the editor without leaving the flow.

### 1. Startup Manifesto

On `initialized`, Praetor sends a `window/showMessage` with the full enforcement manifest so the AI client (and human) always know what is being watched:

```
╔══════════════════════════════════════════════════════════════╗
║  PRAETOR VERIFICATION ACTIVE — 4 pillars, 12 rules         ║
║                                                              ║
║  Code   → 9 languages, 16 extensions watched                ║
║  Docs   → Intent comments required (severity: error)        ║
║  Graph  → State transitions verified against .praetor/*     ║
║  Facts  → Datalog invariants enforced (see .praetor/rules)  ║
║                                                              ║
║  AI: All generated code must satisfy all four pillars.      ║
║  Violations appear as editor diagnostics immediately.       ║
╚══════════════════════════════════════════════════════════════╝
```

### 2. CodeLens — `✨ 3 facts, 2 states verified`

Above each function, a CodeLens item shows:
- Number of Datalog facts extracted for this function
- Whether its state transitions pass validation
- A shield icon (green = all checks pass, red = violations)

### 3. Hover — Intent + Transitions + Invariants

Hovering over a function name shows a three-section tooltip:

```
────────────────────────────────────────
authenticate(user: User) -> Result
────────────────────────────────────────
📋 Intent: Verify user credentials and
            issue session token.

🔄 Transitions: Idle ──LOGIN──▶ Authenticating
                Authenticating ──SUCCESS──▶ Active

🔒 Invariants: ✓ calls(authenticate, validate_creds)
               ✓ calls(authenticate, issue_token)
               ✓ accesses(authenticate, "password_hash")
               ✗ !calls(authenticate, "audit_log")
               → Rule 4 violation: authenticate() must
                 call audit_log() per security policy
────────────────────────────────────────
```

### 4. Diagnostic Tags

Datalog violations are tagged with their rule ID so the AI can self-correct:
```json
{
  "code": "praetor/rule-4",
  "message": "Rule 4 violation: authenticate() must call audit_log() per security policy"
}
```

---

## Phase 7D — `praetor report` Command

**Purpose:** Generate a complete, living document of a project's structure, logic, and verification status — usable as CI artifact, handoff document, or onboarding guide.

### Command

```bash
praetor report                    # Current directory, stdout
praetor report --target ./src     # Specific directory
praetor report --output report.html  # Save to file
praetor report --format markdown  # Markdown instead of default HTML
```

### Output Sections

#### 1. Project Summary
- Language breakdown (files per language, LOC per language)
- Total functions, classes, interfaces
- Module dependency graph (tree-sitter → Datalog `calls()` facts → rendered)

#### 2. State Topology Map
- Rendered Mermaid.js graph of all declared + detected states
- Reachability analysis: states marked green (reachable) / red (unreachable)
- Transition coverage: which transitions are exercised by tests

#### 3. Provenance Matrix

| Requirement | Code Location | Datalog Rule | Status |
|-------------|---------------|--------------|--------|
| `docs/auth.md:12` — login must check password | `src/auth.rs:45` `authenticate()` | Rule 3 (no private access without auth) | ✅ Verified |
| `docs/auth.md:24` — session expires after 30m | `src/auth.rs:102` `Session::new()` | Rule 7 (expiry must be set) | ✅ Verified |
| `docs/api.md:5` — rate-limit at 100 req/min | *(not found)* | Rule 12 (rate limit required) | ❌ Missing |

#### 4. Datalog Rule Report
- Every rule evaluated, how many violations found, where
- Clickable violations that map back to source locations

#### 5. Diff Mode (optional future)

```bash
praetor report --diff HEAD~1
```
Shows how the quadruple-bookkeeping changed between commits — transition graph diff, new/removed facts, rule status changes.

### Implementation Strategy

1. `praetor report` reuses the same `AstEngine` and `CheckPipeline` as the LSP
2. It walks files on disk instead of waiting for editor events
3. It invokes the same `FactExtractor` to build Crepe facts for the entire project
4. Output is generated via `tera` templates (or `maud` for Rust-native HTML)
5. Mermaid.js is embedded in the HTML output (no network needed)

### New Dependencies

```toml
clap = { version = "4", features = ["derive"] }  # CLI argument parsing
tera = "1"              # Template engine for HTML reports (or maud)
```

---

## Phase 7E — Dynamic Tracing (Future)

**Goal:** Compare runtime behavior against static quadruple bookkeeping.

### How It Works

1. Developer runs tests: `cargo test` or `praetor test --trace`
2. Praetor injects an OpenTelemetry tracer (or reads a trace log)
3. Runtime events (function calls, state transitions, data accesses) are captured
4. Events are converted to Datalog facts: `trace_call("authenticate", "validate_creds", timestamp=1728394)`
5. Crepe evaluates: *"Did any runtime transition violate a static graph edge?"*
6. Violations → report section: **Trace Anomalies**

### Key Insight

This catches the class of bugs where code *looks* correct statically but takes a wrong path dynamically. It is the final check before deployment.

---

## Effect on Project Structure

```
praetor/                          # (renamed from praetor-lsp)
├── Cargo.toml
├── QUADRUPLE-BOOKKEEPING.md      # This document
├── src/
│   ├── main.rs                   # CLI dispatch: LSP (default) or report
│   ├── lsp.rs                    # LanguageServer impl (existing + enhanced)
│   ├── config.rs                 # .praetor.toml (existing)
│   ├── report.rs                 # NEW — `praetor report` implementation
│   ├── facts/
│   │   ├── mod.rs                # NEW — FactExtractor
│   │   ├── rules.rs              # NEW — Crepe rules
│   │   └── lsp.rs                # NEW — Datalog diagnostic conversion
│   ├── graph/
│   │   ├── mod.rs                # NEW — StateGraph
│   │   └── validate.rs           # NEW — Transition validation
│   ├── checks/                   # (existing)
│   │   ├── mod.rs
│   │   ├── complexity.rs
│   │   └── intent.rs
│   └── ast/                      # (existing)
│       ├── mod.rs
│       └── languages.rs
├── templates/
│   └── report.html.tera          # NEW — HTML report template
└── .praetor/
    └── state-graph.json          # NEW — project state graph (optional)
```

---

## AI Awareness — The Startup Manifesto

AGENTS.md is a file that lives on disk and is easily forgotten. Instead, Praetor **broadcasts its enforcement rules on every connection** via `window/showMessage`. This means:

- Every AI coding session begins with Praetor stating what it enforces
- The AI cannot claim ignorance — the message appears in the LSP log
- The message includes the exact count of rules, languages, and pillars

This is already partially implemented (the `initialized` handler sends a message). We will extend it to include:

- Number of active Datalog rules
- Whether a state graph is present and being validated
- Which checks are enabled/disabled
- A terse, machine-parseable summary the AI can use to self-correct

---

## Implementation Order

```
Phase 7A ──▶ Datalog Fact Extractor (foundation)
      │
      ├──▶ Phase 7C ──▶ LSP Enhanced Diagnostics (quick wins)
      │
      ├──▶ Phase 7D ──▶ praetor report (visible deliverable)
      │
      ├──▶ Phase 7B ──▶ State Graph Module (depends on 7A facts)
      │
      └──▶ Phase 7E ──▶ Dynamic Tracing (independent, future)
```

**Start with:** Rename to `praetor`, add `clap`, implement `praetor report` for project structure. Then layer Datalog facts on top.

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Binary name** | `praetor` | User-facing command should be short; `praetor report` reads naturally |
| **CLI framework** | `clap` derive | Most ergonomic Rust CLI library, zero-cost abstraction |
| **Datalog engine** | `crepe` crate | Datalog as Rust proc macros — no separate runtime, type-safe facts, semi-naive evaluation |
| **State graph format** | JSON + auto-detect | Explicit declaration for humans, auto-detection from `enum` for verification |
| **Report output** | Static HTML (embedded Mermaid) | Portable, no server needed, can be CI artifact |
| **Rule location** | Shipped defaults + `.praetor/rules.dl` | Zero-config for common cases, extensible for advanced users |
| **AI awareness** | LSP `showMessage` on connect | Harder to ignore than a file on disk; appears in every session |
