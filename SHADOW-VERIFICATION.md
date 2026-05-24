# Shadow Verification: Performance-Gated Refactoring

## The Problem

Praetor emits diagnostics like "10 parameters — consider splitting" or
"cyclomatic complexity 23 — refactor". Some of these are genuinely good
advice, but others are **context-dependent**: flattening params into a
struct or splitting a hot AST walker into tiny functions can regress
performance via extra allocations, register spills, and worse icache
locality.

An LLM (or human) blindly following every diagnostic will produce
correct-but-slower code.

## The Guarantee

> **Praetor will not accept a refactoring that regresses performance
> beyond a configurable threshold.**

No inline exceptions (`// praetor:ignore`), no LLM discretion, no
human judgement calls. The benchmark is the single source of truth.

---

## How It Works

### 1. Prerequisite: declaring a function as `perf_critical`

Perf-critical functions are declared in `.praetor/benchmarks.toml`:

```toml
[benchmarks.collect_facts]
path = "src/facts/mod.rs"
function = "collect_facts"
params = 10                # current count — triggers param-count warning
perf_critical = true       # shadow test required before any refactor

[benchmarks.check_architecture]
path = "src/checks/architecture.rs"
function = "check_architecture"
cyclomatic = 23
perf_critical = true

[benchmarks.apply_incremental_change]
path = "src/lsp.rs"
function = "apply_incremental_change"
nesting_depth = 13
perf_critical = true

[benchmarks.install_tool]
path = "src/downloader.rs"
function = "install_tool"
line_length = 119
perf_critical = true
```

A function is `perf_critical` if:
- It is called on the hot path (every keystroke, every file save)
- It allocates or transforms significant data
- It is recursive or deeply nested with early-return guards

Praetor **auto-suggests** `perf_critical = true` for functions that are
both flagged **and** called inside LSP request handlers.

---

### 2. Proposing a refactoring

When an LLM (or human) proposes a fix for a flagged function, they
write a **shadow function** alongside the original:

```rust
// Original (flagged: 10 params)
fn collect_facts(
    node: Node, lang: &LanguageConfig, source: &[u8],
    sym: &mut SymbolTable,
    calls: &mut Vec<(u32,u32)>, accesses: &mut Vec<(u32,u32)>,
    declares: &mut Vec<(u32,u32)>, annotated: &mut Vec<u32>,
    param_counts: &mut Vec<(u32,u32)>,
    positions: &mut HashMap<u32,(u32,u32)>,
) { ... }

// Shadow (refactored: 2 params via FactContext)
#[praetor::shadow]
fn collect_facts_v2(
    node: Node,
    ctx: &mut FactContext,
) {
    // same logic, but reads/writes through ctx
}
```

The `#[praetor::shadow]` proc-macro attribute:
1. Registers the shadow function in the benchmark harness
2. Links it to the original via the `.praetor/benchmarks.toml` entry
3. Generates a microbenchmark that calls both versions with identical input

---

### 3. Running the comparison

```bash
praetor verify --shadow src/facts/mod.rs
```

This:
1. Extracts the original function and its shadow
2. Builds a standalone benchmark binary with identical randomized inputs
3. Runs each version 10,000+ times
4. Reports:

```
collect_facts:         10 params → 2 params  (flagged)
collect_facts_v2:      2 params               (proposed fix)

  original:  2.34 µs/iter  (±0.02)
  shadow:    2.41 µs/iter  (±0.03)  ← 3% slower

  ❌ REJECTED — shadow is slower than original.
     Threshold: +2%. Actual: +3%.
     Suggestion: keep the original; the struct allocation cost
     outweighs the readability benefit on this hot path.
```

Or, if the shadow is faster:

```
  original:  2.34 µs/iter  (±0.02)
  shadow:    2.10 µs/iter  (±0.01)  ← 10% faster

  ✅ ACCEPTED — shadow outperforms original.
     Shadow promoted to `collect_facts`. Original archived.
```

---

### 4. Verification workflow

```
┌─────────────────────────────────────────────────────┐
│ 1. Diagnostic fires on perf_critical function        │
│    "collect_facts has 10 params (max 6)"            │
└─────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────┐
│ 2. LLM/human writes shadow function                 │
│    #[praetor::shadow] with refactored logic          │
└─────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────┐
│ 3. praetor verify --shadow <file>                   │
│    - Builds benchmark harness                        │
│    - Runs original vs shadow × 10,000 iters         │
│    - Compares mean, variance, p-value               │
└─────────────────────────────────────────────────────┘
                        │
                        ├── Faster  → ✅ Promoted
                        ├── Equal   → ✅ Promoted (noise)
                        └── Slower  → ❌ Rejected
                                      Warning stays.
                                      Shadow is discarded.
```

---

## Edge Cases

### Noise in benchmarks
If `|shadow - original| / original < 3%`, treat as equal. The
threshold is configurable in `.praetor.toml`:

```toml
[verification]
perf_threshold_pct = 3    # reject if shadow is >3% slower
min_iterations = 10000
```

### Functions with no benchmark input
Some functions (e.g., `detect_os`) have no interesting input space.
Praetor skips shadow verification for these and treats the warnings as
pure readability advice.

### Functions that can't be shadowed (I/O, side effects)
Praetor auto-detects functions that call `std::process::Command`,
`std::fs`, or `tokio::io`. These are marked `perf_critical = false`
by default since microbenchmarks can't meaningfully measure them.

### Shadow cannot compile
If the shadow function doesn't compile, Praetor reports:

```
❌ SHADOW COMPILATION FAILED
   src/facts/mod.rs:1:1 — missing field `calls` in `FactContext`
```

The fix is rejected automatically. The LLM must fix the shadow and
re-run `praetor verify`.

---

## What This Replaces

| Old approach | Problem | Shadow verification |
|-------------|---------|-------------------|
| `// praetor:ignore` | Allows cheating; ignores forever | No inline exceptions. Prove it with a benchmark or keep the warning. |
| Blind LLM fix | LLM applies every diagnostic, regressing hot paths | LLM must write a shadow or the fix is rejected. |
| Human code review | Subjective; reviewer may not notice perf regression | Objective: numbers decide, not opinions. |

---

## What This Costs

- **One proc-macro crate** (`praetor-derive`) for `#[praetor::shadow]`
- **One benchmark harness** (wraps `divan` or `criterion`)
- **One CLI command** (`praetor verify --shadow`)
- **Time**: ~10 seconds per verification (build + run 10k iterations)

The key insight: **you don't pre-benchmark everything.** You only build
a shadow when someone proposes to act on a diagnostic. Uncontested
warnings cost nothing.