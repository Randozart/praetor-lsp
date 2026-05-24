# Lessons from Dogfooding

## Lesson 1: Draconic by default, shadow to escape

**The problem:** An LLM reviewing Praetor's own diagnostics saw `else_clause` in
`DECISION_KINDS` and proposed removing it — arguing that `else` is "not a real
decision" because it's a tree-sitter structural container and the nested
`if_expression` already accounts for the complexity.

**The mistake:** That analysis was technically correct per the cognitive
complexity standard, but it missed the point. Praetor's purpose is to be
_draconian_: it should push back harder than average, forcing the LLM to either
refactor to a demonstrably cleaner pattern or write a shadow benchmark and
prove the original is worth keeping.

Removing `else_clause` would have weakened Praetor. Instead:

1. **Keep the strict rule** — `else_clause` stays in `DECISION_KINDS`
2. **Give actionable guidance** — the diagnostic message now says
   "consider early returns instead of else-if chains"
3. **The shadow escape hatch handles the edge case** — if early returns truly
   can't match the performance of `cfg!()` chains (which are compile-time
   eliminated), write a shadow, benchmark, silence

**Lesson:** When a rule feels too strict, don't soften the rule — strengthen
the actionable guidance and verify the escape hatch works.

## Lesson 2: Thresholds are not the problem

Initial reaction to `detect_os` at cognitive 21: "the threshold is too low."
The fix wasn't raising the threshold — it was realizing the function can be
written with early returns instead of if-else chains. The threshold stayed at
15. The code should change, not the metric.

**Lesson:** When a metric fires on code that "looks fine," the code should
change, not the metric. Only adjust thresholds after observing patterns across
multiple projects, not because a single function in your own codebase flags.

## Lesson 3: Hash validation needs the CI machine

The shadow registry stores hashes to detect stale entries (function changed
since verification). On a developer laptop where the same person writes both
the code and the benchmark, the hash check is just noise — the benchmark was
run two seconds ago, of course the hash matches.

The hash check becomes valuable when:
- CI runs the benchmark (not the developer)
- The LLM proposes code changes (not the human)
- The registry is shared across a team

Implement hash validation when the CI machine is the sole registry writer.
Before that, it's premature infrastructure.

## Lesson 4: Scaffold generality

The `praetor verify --shadow` scaffold assumes functions take typed parameters.
Zero-argument functions, void functions, and side-effect-only initialization
functions don't fit. The scaffold needs to detect the function signature and
generate appropriate test input stubs.

## Lesson 5: The AI's job is to find leverage, not complain

The original response to "dogfood this" was a list of excuses: thresholds need
tuning, scaffold needs refinement, hash validation is broken. None of those
blocked the actual work. The real fix was a one-line change (reinstating
`else_clause`) and a format string update (actionable hint). The AI should
identify the actual bottleneck, fix it, and keep moving — not philosophize
about busywork.