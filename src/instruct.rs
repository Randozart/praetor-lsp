/// Instructions for AI agents using Praetor.
///
/// Run `praetor instruct` to print this at any time.
pub const INSTRUCT_TEXT: &str = r#"═══════════════════════════════════════════════════════════
                    PRAETOR — AI INSTRUCTIONS
═══════════════════════════════════════════════════════════

Praetor is a quadruple-bookkeeping verification LSP that
improves code quality through four pillars:

  1. COMPLEXITY  — Big-O, cyclomatic, cognitive metrics
  2. INTENT      — Every function needs a comment explaining why
  3. STATE GRAPH — Code transitions must match declared state machine
  4. DATALOG     — Invariants enforced via Datalog rules

WHEN YOU SEE A PRAETOR DIAGNOSTIC:

  • Read the message — it tells you exactly what's wrong
  • Fix the issue — split large functions, add comments,
    reduce nesting, etc.
  • If stuck, ask: "Run praetor report" for a full project
    overview of all violations.

HOW TO USE:

  praetor lsp         Run as LSP server (auto via editor)
  praetor report      Generate full verification report
  praetor validate    CI gate — exits 1 on unproven diagnostics
  praetor verify      Shadow-benchmark original vs optimized code
  praetor init        Initialize Praetor in your project
  praetor setup       Install external dependencies
  praetor instruct    Show this message

RULES FOR AI AGENTS:

  1. Do not suppress diagnostics without fixing the root cause.
  2. When refactoring, preserve intent comments.
  3. If a function exceeds thresholds, split it into smaller
     single-responsibility functions.
  4. Use early returns to reduce cognitive complexity.
  5. Every public function needs an intent comment.

═══════════════════════════════════════════════════════════
"#;

pub fn print_instruct() {
    println!("{}", INSTRUCT_TEXT);
}

/// Append the praetor instruct hint to a diagnostic message.
pub fn with_instruct_hint(message: &str) -> String {
    format!(
        "{} — Run `praetor instruct` for detailed instructions on how AI should use Praetor",
        message
    )
}
