# Praetor — Full Development Plan (Phases 1–7)

## Overview

Single Rust binary (`praetor`) providing quadruple-bookkeeping verification to any
LSP-capable client. Four mutually-reinforcing pillars:

| Pillar | Format | Status |
|--------|--------|--------|
| **Code** | 9 languages, 16 extensions via tree-sitter | ✅ Phase 1 |
| **Docs** | Intent comments enforced (error/warning/hint) | ✅ Phase 3 |
| **Facts** | Crepe Datalog engine, 5 built-in rules | ✅ Phase 7A |
| **State Graph** | `.praetor/state-graph.json` validation | ❌ Phase 7B |

---

## Phase 1 — Rust Skeleton + Core AST Engine ✅

- [x] `cargo init` with `tower-lsp`, `tree-sitter`, `serde`, `toml`
- [x] LSP handlers: initialize, shutdown, didOpen, didChange, didSave, didClose
- [x] tree-sitter parsers for 9 languages (Python, JS, TS, TSX, Go, C, C++, Rust, Java)
- [x] InlayHint provider for complexity labels
- [x] Incremental text sync engine
- [x] Binary renamed to `praetor` with `clap` CLI subcommands

## Phase 2 — Complexity Checks ✅

- [x] Big-O classification: O(1), O(n), O(n·m), O(n²), O(n²·m), O(2ⁿ), O(n^k)
- [x] Loop nesting depth detection (for/while/do/foreach)
- [x] Recursive call detection (self-recursion via AST)
- [x] Linear operation detection inside loops (`.indexOf()`, `.find()`, etc.)
- [x] 16 file extensions across 9 languages

## Phase 3 — Strict Intent Mode ✅

- [x] Comment node detection before function/class declarations
- [x] Language-aware comment type matching
- [x] Configurable severity: error/warning/hint
- [x] Exempt pattern matching via regex
- [x] `.praetor.toml` auto-discovery and parsing

## Phase 4 — Auto-Download Manager ❌

- [ ] Semgrep binary download, verification, caching
- [ ] SonarLint JAR download + lifecycle management
- [ ] Infer binary download (Linux only)
- [ ] `~/.praetor-lsp/` directory setup on first run
- [ ] Version checking for cached binaries
- [ ] Graceful skip if download fails (info-level log)

**Key decisions:**
- Downloads in background on first `praetor` start, not at compile time
- Hash-verified downloads from official GitHub releases / Maven Central
- Cache at `~/.praetor-lsp/bin/` and `~/.praetor-lsp/lib/`

## Phase 5 — Formal Verification Bridge ❌

- [ ] `src/bridge/semgrep.rs` — Semgrep child process + JSON output parser
- [ ] `src/bridge/infer.rs` — Infer `infer-out/report.json` parser
- [ ] `src/bridge/sonarlint.rs` — SonarLint JAR process manager
- [ ] Unified diagnostic format across all bridge tools
- [ ] `rules/` directory with bundled Semgrep rule packs

### Bridge integration points

| File | Purpose |
|------|---------|
| `src/bridge/mod.rs` | Trait + dispatch logic |
| `src/bridge/semgrep.rs` | `SemgrepBridge` — runs semgrep, parses JSON |
| `src/bridge/infer.rs` | `InferBridge` — reads `infer-out/report.json` |
| `src/bridge/sonarlint.rs` | `SonarLintBridge` — manages JAR process |

All bridges emit the same `CheckDiagnostic` format so they flow through the
existing `CheckPipeline` and appear as LSP diagnostics automatically.

### Bundle rules

```
rules/
├── python/
│   ├── sql-injection.yaml
│   ├── hardcoded-secrets.yaml
│   └── unsafe-deserialization.yaml
├── javascript/
│   ├── xss.yaml
│   ├── unhandled-promise.yaml
│   └── react-hooks.yaml
└── go/
    ├── sql-injection.yaml
    └── unsafe-exec.yaml
```

## Phase 6 — Polish + Distribution ❌

- [ ] `.github/workflows/release.yml` — cross-compile for Linux/macOS/Windows
- [ ] `cross` targets: x86_64-linux, aarch64-linux, x86_64-macos, aarch64-macos, x86_64-windows
- [ ] Install script: `curl -fsSL https://.../install.sh | sh`
- [ ] Homebrew tap: `brew install praetor`
- [ ] Comprehensive README with examples of all checks
- [ ] `cargo install praetor` on crates.io

## Phase 7A — Datalog Fact Extractor ✅

- [x] Crepe Datalog engine with 5 input + 1 output relations
- [x] Symbol table for string interning
- [x] AST walker extracting: calls, accesses, declarations, annotations
- [x] 5 built-in rules (unauthenticated access, unreachable handlers, unused variables, excessive params, delegated data leaks)
- [x] Wired into CheckPipeline → LSP diagnostics
- [x] Position tracking (violations point to correct line)

### Relations

```
@input  Call(u32, u32)         // caller_id, callee_id
@input  Access(u32, u32)       // fn_id, resource_id
@input  Declares(u32, u32)     // fn_id, var_id
@input  Annotated(u32)         // fn_id (has doc comment)
@input  ParamCount(u32, u32)   // fn_id, count

@output Violation(u32, u32, u32) // rule_id, fn_id, detail_id
```

### Built-in Rules

| # | Rule | Severity |
|---|------|----------|
| 1 | Private data access without `authenticate()` or `log_access()` | Error |
| 2 | Documented function with zero callers (unreachable) | Hint |
| 3 | Declared variable never read | Hint |
| 4 | Function with >5 parameters | Warning |
| 5 | Calls a callee that accesses private data without auth | Error |

## Phase 7B — State Graph Module ❌

**Goal:** Two-way validation between a declared state graph and actual code.

### New files

| File | Purpose |
|------|---------|
| `src/graph/mod.rs` | `StateGraph` struct — load, validate, query |
| `src/graph/schema.rs` | Graph representation types |
| `src/graph/validate.rs` | Walk AST call sites, check against allowed transitions |

### Graph format (`.praetor/state-graph.json`)

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

### Validation rules

1. Every call to a state-transition function must pass a valid target state
2. No function may read data in a state it did not authenticate to reach
3. Every declared state must be reachable from the initial state
4. No declared state may be unreachable (dead state detection)

### Auto-detection from enums

For Rust, parse `enum`-based state machines:

```rust
// Generates facts: state("Pending"), state("Active"), state("Suspended")
// transition("Pending", "activate", "Active")
enum AccountState { Pending, Active, Suspended }
```

Auto-detected graph is compared against declared graph. If they differ → diagnostic.

### Crepe integration

Existing `transition(From, Event, To)` relation feeds into rules:

```datalog
// Illegal transition: code calls transition_to("Active") but
// current state "Idle" has no edge to "Active"
Violation(6, f, 0) <-
    Call(f, "transition_to"),
    !Transition(_, "transition_to", _).
```

## Phase 7C — LSP Enhanced Diagnostics

- [x] **Startup Manifesto** — `window/showMessage` on connect with enforcement rules
- [x] **CodeLens** — per-function verification status (✅ verified / ⚠️ warnings / ⛔ errors)
- [x] **Diagnostic rule IDs** — all diagnostics tagged with their rule source
- [ ] **Enhanced Hover** — show intent + transitions + invariants on function hover
- [ ] **Diagnostic code actions** — "Add intent comment" quick-fix for missing docs

### Hover provider spec

Hovering over a function name returns markdown:

```
📋 **Intent:** Verify user credentials and issue session token.

🔄 **Transitions:** Idle ──LOGIN──▶ Authenticating
                   Authenticating ──SUCCESS──▶ Active

🔒 **Invariants:** ✓ calls(authenticate, validate_creds)
                   ✓ calls(authenticate, "password_hash")
                   ✗ !calls(authenticate, "audit_log")
                   → Rule 4 violation
```

The hover constructs this from the Datalog facts + state graph + config.

## Phase 7D — `praetor report` Command ✅

- [x] `praetor report --target ./dir` — walks directory, runs all checks
- [x] Markdown output (stdout)
- [x] HTML output (`--output report.html`)
- [x] Project summary (files, lines, functions per language)
- [x] Per-file diagnostic listing with line numbers
- [x] Verification status section
- [ ] Mermaid.js state graph rendering in HTML
- [ ] Provenance matrix (requirement → code → rule)
- [ ] Datalog rule execution summary
- [ ] `--diff` mode comparing against git baseline

## Phase 7E — Dynamic Tracing ❌ (Future)

**Goal:** Compare runtime behavior against static quadruple bookkeeping.

### How it would work

1. Developer runs tests: `cargo test` or `praetor test --trace`
2. Praetor injects an OpenTelemetry tracer (or reads a trace log file)
3. Runtime events (function calls, state transitions, data accesses) are captured
4. Events are converted to Datalog facts: `trace_call("authenticate", "validate_creds")`
5. Crepe evaluates: *"Did any runtime transition contradict a static graph edge?"*
6. Violations appear in a report section: Trace Anomalies

**Catch:** Code that looks correct statically but takes a wrong path dynamically.

### Dependencies (future)

```toml
opentelemetry = "0.27"
opentelemetry-otlp = "0.27"
```

---

## Architecture SOLID Heuristics (Planned)

Native AST pattern matching for design-level issues:

| Heuristic | Detection | Status |
|-----------|-----------|--------|
| God Object | Class >300 lines or >10 public methods | ❌ |
| Feature Envy | Method calls more getters/setters on other classes than its own | ❌ |
| Shotgun Surgery | Function modifies >3 different data structures | ❌ |
| Divergent Change | Class changed for multiple different reasons | ❌ |
| Excessive Coupling | >8 direct dependencies in a module | ❌ |
| Deep Inheritance | Hierarchy depth >4 | ❌ |

### New file: `src/checks/architecture.rs`

Walk AST looking for class definitions, method calls, field accesses. Count
coupling metrics. Emit `CheckDiagnostic` when thresholds are exceeded.

---

## Implementation Order — Priority Matrix

| Priority | Phase | What | Est. Effort | Value |
|----------|-------|------|-------------|-------|
| P0 | 7C | Hover provider | 2 days | High — closes the feedback loop |
| P0 | 7B | State Graph Module | 3 days | High — third pillar of quadruple bk |
| P1 | 5 | bridge/ module (Semgrep) | 3 days | High — security scanning |
| P1 | 4 | Auto-Download Manager | 2 days | Required for Phase 5 |
| P1 | 7D | Report improvements (Mermaid, provenance) | 2 days | Medium |
| P2 | 2 | Metrics (cyclomatic, cognitive, line/param) | 1 day | Medium |
| P2 | 5 | bridge/ (Infer, SonarLint) | 4 days | Medium |
| P2 | 6 | Polish + Distribution | 3 days | Medium |
| P3 | 7E | Dynamic Tracing | 5 days | Low (future) |
| P3 | — | Architecture/SOLID heuristics | 3 days | Low |

---

## Project Structure (Target)

```
praetor/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI: LSP (default), report
│   ├── lsp.rs               # LanguageServer impl + CodeLens + Hover
│   ├── config.rs            # .praetor.toml parser
│   ├── report.rs            # praetor report command
│   ├── downloader.rs        # Auto-download manager (Phase 4)
│   ├── ast/
│   │   ├── mod.rs           # AstEngine + AST utilities
│   │   └── languages.rs     # Language configs
│   ├── checks/
│   │   ├── mod.rs           # CheckPipeline orchestrator
│   │   ├── complexity.rs    # Big-O analysis
│   │   ├── intent.rs        # Strict Intent Mode
│   │   ├── metrics.rs       # Cyclomatic/cognitive metrics ❌
│   │   ├── architecture.rs  # SOLID heuristics ❌
│   │   └── facts.rs         # Datalog fact check wrapper ✅
│   ├── facts/
│   │   └── mod.rs           # Crepe Datalog engine ✅
│   ├── graph/
│   │   ├── mod.rs           # StateGraph ❌
│   │   └── validate.rs      # Transition validation ❌
│   └── bridge/
│       ├── mod.rs           # Bridge trait + dispatch ❌
│       ├── semgrep.rs       # Semgrep runner ❌
│       ├── infer.rs         # Infer parser ❌
│       └── sonarlint.rs     # SonarLint manager ❌
├── rules/                   # Semgrep rule packs ❌
├── scripts/                 # Python prototypes (reference)
│   ├── complexity_lsp.py
│   └── infer_lsp.py
└── lib/                     # Auto-downloaded binaries
    └── sonarlint-language-server.jar
```
