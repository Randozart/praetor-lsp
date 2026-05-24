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

## The Three-Gate Pipeline

A shadow function must pass three sequential gates before any
diagnostic is silenced:

```
┌─ GATE 1: IO EQUIVALENCE ────────────┐
│ Fuzz both functions with identical   │
│ inputs, assert_eq! outputs           │
│ Fail → ❌ "shadow changes behavior"  │
└──────────────────────────────────────┘
                   ✅
┌─ GATE 2: METRIC IMPROVEMENT ────────┐
│ Run full CheckPipeline on shadow.    │
│ Must improve on the flagged metric.  │
│ Fail → ❌ "shadow doesn't fix flag"  │
└──────────────────────────────────────┘
                   ✅
┌─ GATE 3: BENCHMARK + TIEBREAKER ────┐
│ original vs shadow, 500k iters       │
│                                      │
│ Shadow faster  → ✅ PROMOTE shadow   │
│ Original faster → ✅ ORIGINAL kept   │
│                  → warning silenced  │
│ Tie (within 3%) → compare all other  │
│   CheckPipeline metrics across both  │
│   → better aggregate wins            │
│   → warning silenced if original     │
└──────────────────────────────────────┘
```

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

### 2. Generating the benchmark scaffold

```bash
praetor verify --shadow src/lsp.rs
```

This scans for `praetor-shadow:` comments and generates an inline
benchmark module with:
- Test input stubs (user fills in realistic test data)
- IO equivalence checks (runs both functions on same inputs)
- Metric comparison (runs CheckPipeline on both)
- Benchmark loop (times both)
- Registry writer (writes `.praetor/shadow-results.json`)

### 3. Running the verification

```bash
cargo test bench_apply_incremental_change -- --nocapture
```

Output:

```
=== Shadow Verification: apply_incremental_change ===

── Gate 1: IO Equivalence ──
  Testing 6 inputs... all match ✅

── Gate 2: Metric Improvement ──
  original: nesting 13, cognitive 44, cyclomatic 1, param_count 3
  shadow:   nesting  5, cognitive 12, cyclomatic 1, param_count 3
  ✅ shadow improves on flagged metric (nesting: 13→5)

── Gate 3: Benchmark ──
  original: 1184.1 ns/op
  shadow:   1189.4 ns/op
  ratio:    1.004× (within 3% threshold)

  → TIE — comparing aggregate metrics
    original: 4 flags (nesting, cognitive, lines, params)
    shadow:   2 flags (nesting, lines)
  ✅ shadow wins tiebreaker

  ✅ PROMOTED — shadow replaces original
```

Or, if the original is faster:

```
  original: 1184.1 ns/op
  shadow:   2410.1 ns/op
  ratio:    2.04× SLOWER

  → ORIGINAL WINS — warning silenced
  → Entry written to .praetor/shadow-results.json
  → Praetor will suppress future diagnostics for this function
```

### 4. Registry format

Results are stored in `.praetor/shadow-results.json`:

```json
{
  "apply_incremental_change": {
    "original_hash": "sha256-abc...",
    "shadow_hash": "sha256-def...",
    "winner": "original",
    "ratio": 1.004,
    "improvement": {
      "nesting": {"before": 13, "after": 5},
      "cognitive": {"before": 44, "after": 12}
    },
    "suppressed_diagnostics": ["praetor/metrics/nesting", "praetor/metrics/cognitive"],
    "verified_at": "2026-05-24T14:00:00Z"
  }
}
```

### 5. Diagnostic suppression

When Praetor's CheckPipeline runs, it checks the registry before
emitting each diagnostic. If the function + diagnostic type appears
in the registry with a valid hash and `winner == "original"`, the
diagnostic is **downgraded to `HINT`** (not hidden — visible but
not blocking).

If the original function's source changes, the hash no longer matches
and the registry entry is invalidated — warnings return until
re-verified.

---

## Edge Cases

### Noise in benchmarks
If `|shadow - original| / original < 3%`, treat as tie. Threshold
configurable in `.praetor.toml`:

```toml
[verification]
perf_threshold_pct = 3
min_iterations = 50000
```

### Functions with no benchmark input
Some functions (e.g., `detect_os`) have no interesting input space.
Praetor skips shadow verification for these.

### Functions that can't be shadowed (I/O, side effects)
Praetor auto-skips functions that call `std::process::Command`,
`std::fs`, or `tokio::io` since microbenchmarks can't measure them.

### Shadow cannot compile
If the shadow doesn't compile, the test fails at compile time —
no gates reached.

### Registry entry expires
If the original or shadow function body changes (hash mismatch),
the entry is invalid. Re-run `praetor verify --shadow` to renew.

---

## What This Replaces

| Old approach | Problem | Shadow verification |
|-------------|---------|-------------------|
| `// praetor:ignore` | Allows cheating; ignores forever | Three-gate proof or keep the warning |
| Blind LLM fix | Regresses hot paths | Must pass IO + metric + benchmark gates |
| Human code review | Subjective | Numbers decide, not opinions |

## What This Costs

- **Zero dependencies** — comment-based, no proc-macro, no build-time overhead
- **One CLI command** (`praetor verify --shadow`)
- **Language-agnostic** — works for Rust, Python, JavaScript, Go, C, C++, Java
- **Time**: ~10 seconds per verification (build + run 50k iterations)
- **Registry**: a single JSON file in `.praetor/`
