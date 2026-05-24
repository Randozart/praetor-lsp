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

### 1. Marking a shadow function

The LLM (or human) writes a refactored version of a flagged function and
precedes it with a `praetor-shadow:` comment declaring the original:

```rust
// praetor-shadow: original=collect_facts
fn collect_facts_v2(node: Node, ctx: &mut FactContext) {
    // refactored logic
}
```

The comment syntax is **language-agnostic** — it works everywhere:

| Language | Comment style | Example |
|----------|--------------|---------|
| Rust, Go, C, C++, Java, JS/TS | `//` | `// praetor-shadow: original=foo` |
| Python | `#` | `# praetor-shadow: original=foo` |
| Any | `/* */` | `/* praetor-shadow: original=foo */` |

The `original=` value is the name of the function being refactored.
If omitted, the tool guesses it by stripping `_shadow`, `_v2`, or `_v3`
suffixes from the shadow function name.

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
// praetor-shadow: original=collect_facts
fn collect_facts_v2(
    node: Node,
    ctx: &mut FactContext,
) { ... }
```

The `// praetor-shadow:` comment is a language-agnostic marker that
`praetor verify --shadow` discovers at runtime by scanning the source
file line by line. It works for Rust, Python, JavaScript, Go, C, C++,
Java — any language that uses `//`, `#`, or `/* */` comments.

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
│    // praetor-shadow: original=collect_facts        │
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

- **Zero dependencies** — comment-based, no proc-macro, no build-time overhead
- **One CLI command** (`praetor verify --shadow`)
- **Language-agnostic** — works for Rust, Python, JavaScript, Go, C, C++, Java
- **Time**: ~10 seconds per verification (build + run 10k iterations)

The key insight: **you don't pre-benchmark everything.** You only build
a shadow when someone proposes to act on a diagnostic. Uncontested
warnings cost nothing.