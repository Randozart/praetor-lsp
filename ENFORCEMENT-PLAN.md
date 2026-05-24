# Praetor Enforcement Implementation Plan

Making Praetor genuinely useful and inescapable on any random codebase.

---

## Phase 0 — Fix the Foundation

### 0.1 — Metrics inflator bug

**File:** `src/checks/metrics.rs`
**Change:** Remove `kind == "block" || kind == "body"` from both `compute_cognitive` (line ~170) and `max_nesting_depth` (line ~200).

**Problem:** Every brace-delimited scope counts as a nesting level, then every
if/for/while body inside it adds another. A 3-if function reports cognitive ~21
instead of ~3. This is the single reason ~30 diagnostics are noise.

**Impact:** `detect_os` goes from cognitive 21 → 3. `check_metrics` nesting
from 5 → 4. Most nesting/cognitive hints on simple functions disappear.

---

### 0.2 — State graph made opt-in

**File:** `src/config.rs`, `src/checks/mod.rs`
**Change:** Add `StateGraphConfig { enabled: false }` to config. Gate state graph
validation behind `config.state_graph.enabled`.

**Problem:** The state graph module does heuristic substring matching on function
names. On a random codebase with no `.praetor/state-graph.json`, it runs zero
code. But the infrastructure runs on every file. Make the feature explicitly
opt-in so users know it's an opt-in feature.

**Config shape:**
```toml
[state_graph]
enabled = false
path = ".praetor/state-graph.json"
```

---

## Phase 1 — Complete the Active Pillars

### 1.1 — Semgrep bridge producing real output

**Files:** `src/bridge/semgrep.rs`, `src/bridge/mod.rs`

**Problem:** The bridge checks `~/.praetor-lsp/bin/semgrep` which never exists
(the auto-downloader URL is broken). Even if semgrep is installed on PATH,
`is_available()` returns false.

**Changes:**

1. `is_available()` — check both cache path and PATH:
```rust
fn is_available(&self) -> bool {
    let cache = cache_root();
    cache.join("bin").join("semgrep").exists()
        || Command::new("semgrep").arg("--version").output().is_ok()
}
```

2. `run()` — use PATH semgrep when cache binary missing:
```rust
let bin_path = if cache_bin.exists() { cache_bin } else { PathBuf::from("semgrep") };
```

3. Add `tool_is_available(name: &str)` helper to `bridge/mod.rs` to share
   this pattern across Semgrep, Infer, and future bridges.

4. Create `test/fixtures/insecure.py` — a Python file with known bugs
   (SQL injection, hardcoded password) that Semgrep's default rules flag.

**Why Semgrep:** 2,000+ rules for all 9 languages. Single `pip install`.
Structured JSON output already parsed. Fastest path to real diagnostics.

---

### 1.2 — Datalog rules configurable via `.praetor.toml`

**Files:** `src/config.rs`, `src/facts/mod.rs`

**Problem:** Five Crepe rules hardcode `"authenticate"`, `"private_data"`,
`"log_access"`, `"main"`, `"run"` as literal interned strings. These never
fire on real codebases with different naming conventions.

**Config shape:**
```toml
[datalog]
auth_functions = ["authenticate", "authorize", "login", "verify_session"]
private_data_labels = ["private", "secret", "password", "token"]
entry_points = ["main", "run", "start", "handle", "handler"]
log_functions = ["log", "log_access", "audit", "write_log"]
```

**Implementation:**
1. Add `DatalogConfig` struct with those four `Vec<String>` fields, all optional.
2. `FactContext::with_config(cfg: &DatalogConfig)` pre-interns the configured
   names instead of the hardcoded literals.
3. If no config, fall back to current hardcoded defaults.

The five Crepe rules remain unchanged — only the input fact names change.

---

### 1.3 — SonarLint bridge: minimal viability

**File:** `src/bridge/sonarlint.rs`

**Problem:** The SonarLint JAR exists at `~/praetor-lsp/lib/sonarlint-language-server.jar`
(~53 MB) but the bridge returns empty results with a log warning.

**Change:** `is_available()` already returns true when the JAR exists. Document
that SonarLint requires a running LSP subprocess and is not a one-shot CLI.
No code change needed — just the existing stub with accurate `is_available()`.

---

## Phase 2 — The Enforcement Layer

### 2.1 — Registry hash validation

**File:** `src/suppressor.rs`

**Problem:** `is_suppressed()` checks function name + diagnostic source but does
not verify the function body hash. If the function changes after the registry
entry was written, the suppression is stale and silently wrong.

**Change:** Add hash verification:

```rust
pub fn is_suppressed(
    &self,
    function_name: &str,
    diagnostic_source: &str,
    function_body: &str,  // NEW: current source of the function
) -> bool {
    let entry = self.entries.get(function_name)?;
    if entry.winner != "original" { return false; }
    if entry.original_hash != hash_source(function_body) { return false; }
    entry.suppressed_diagnostics.iter().any(|s| diagnostic_source.contains(s))
}
```

Also update `suppress_in_file` to pass the function body source to
`is_suppressed`.

---

### 2.2 — Pre-commit hook

**File:** `scripts/pre-commit.sh` (installed to `.git/hooks/pre-commit`)

**Script:**
```bash
#!/bin/sh
# Generated by `praetor init`. Editing this file voids your warranty.

set -e

# Locate praetor
if command -v praetor >/dev/null 2>&1; then
    PRAETOR=praetor
else
    PRAETOR=~/praetor-lsp/target/debug/praetor
fi

# Generate report
REPORT=$("$PRAETOR" report --target . 2>/dev/null)

# Check for WARNING or ERROR diagnostics
WARNINGS=$(echo "$REPORT" | awk -F'|' '
    /Warning.*praetor/ || /Error.*praetor/ {
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2);
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $4);
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $5);
        printf "%s | %s | %s\n", $2, $4, $5;
    }
')

if [ -n "$WARNINGS" ]; then
    echo ""
    echo "=== Praetor: unproven diagnostics found ==="
    echo "$WARNINGS"
    echo ""
    echo "To commit, you must either:"
    echo "  1. Refactor to satisfy the check"
    echo "  2. Write a shadow function and run the benchmark to prove parity"
    echo ""
    echo "See SHADOW-VERIFICATION.md for the shadow escape hatch."
    echo "=== Commit rejected ==="
    exit 1
fi
```

**Design notes:**
- Uses `awk` not `grep` to produce clean formatted output (POSIX-safe)
- Exits 0 if no unproven WARNING/ERROR diagnostics exist
- Does NOT parse `shadow-results.json` — the report command already filters
  suppressed diagnostics. Any WARNING/ERROR that appears in the report is
  by definition unproven.

---

### 2.3 — `praetor init` command

**New file:** `src/init.rs`
**Modified:** `src/main.rs`

**CLI:**
```
praetor init         # interactive, prompts before overwriting
praetor init --force # non-interactive, overwrites
```

**What it does:**
1. Find project root — look for `Cargo.toml`, `package.json`, `pyproject.toml`,
   `go.mod`, or fall back to CWD.
2. Create `.praetor/` directory if missing.
3. Create `.praetor.toml` if missing (with default config).
4. Create `.praetor/shadow-results.json` if missing (empty `{}`).
5. Check if `.git/hooks/pre-commit` exists:
   - If missing: install `scripts/pre-commit.sh` as the hook.
   - If present: prompt before overwriting (skip with `--force`).
6. Print success message with next steps.

**`src/main.rs` additions:**
```rust
enum Commands {
    Lsp,
    Report { ... },
    Verify { ... },
    Init { force: bool },
    Validate { warn: bool },
}
```

---

### 2.4 — `praetor validate` command

**New file:** `src/validate.rs`
**Modified:** `src/main.rs`

**CLI:**
```
praetor validate           # exit 1 if any WARNING/ERROR diagnostic found
praetor validate --warn    # exit 1 only for ERROR, allow WARNING and below
praetor validate --json    # output results as JSON for CI consumption
```

**What it does:**
1. Runs the full `praetor report` analysis pipeline.
2. Filters diagnostics to unproven ones (the report already suppresses).
3. If `--warn` is set, only ERROR level causes exit 1; WARNING/HINT pass.
4. If `--json` is set, outputs structured JSON instead of human text.
5. Exit code: 0 = pass, 1 = fail (unproven diagnostics at required severity).

This is the CI gate. A GitHub Action runs `praetor validate --warn --json`
and fails the PR if new unproven diagnostics appear.

---

## Phase 3 — Verifiable CI

### 3.1 — GitHub Actions workflow

**New file:** `.github/workflows/verify.yml`

```yaml
name: Praetor verification
on: [pull_request]
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - name: Install semgrep
        run: pip install semgrep
      - name: Build praetor
        run: cargo build
      - name: Run verification
        run: ./target/debug/praetor validate --warn --json
```

### 3.2 — End-to-end smoke test

**New file:** `test/fixtures/insecure.py`

```python
import sqlite3

def get_user(db, user_id):
    query = "SELECT * FROM users WHERE id = " + user_id  # SQL injection
    return db.execute(query).fetchone()

def authenticate(user, password):
    if password == "supersecret":  # hardcoded password
        return True
    return False
```

**New file:** `tests/smoke_test.rs`

```rust
#[test]
fn praetor_produces_diagnostics() {
    // 1. Create temp directory
    // 2. Write insecure.py
    // 3. Run `praetor report --target <dir>`
    // 4. Assert at least 1 WARNING-level diagnostic exists
    // 5. Assert diagnostic source mentions "semgrep" or "datalog" or "metrics"
}
```

---

## Phase 4 — Update AGENTS.md

**File:** `~/AGENTS.md`

Rewrite to reflect enforcement:

- Praetor is an LSP server running on every keystroke
- Pre-commit hook rejects new unproven diagnostics
- CI gate rejects new unproven diagnostics
- The only escape hatch is a shadow benchmark
- No inline exceptions, no human override

---

## File Change Summary

| File | Action | Phase |
|------|--------|-------|
| `src/checks/metrics.rs` | Edit (remove block/body) | 0.1 |
| `src/config.rs` | Edit (add StateGraphConfig, DatalogConfig) | 0.2, 1.2 |
| `src/checks/mod.rs` | Edit (gate state graph) | 0.2 |
| `src/bridge/semgrep.rs` | Edit (PATH fallback) | 1.1 |
| `src/bridge/mod.rs` | Edit (tool_is_available helper) | 1.1 |
| `src/facts/mod.rs` | Edit (configurable Datalog names) | 1.2 |
| `src/suppressor.rs` | Edit (hash validation) | 2.1 |
| `src/init.rs` | **New** | 2.3 |
| `src/validate.rs` | **New** | 2.4 |
| `src/main.rs` | Edit (add Init/Validate commands) | 2.3, 2.4 |
| `scripts/pre-commit.sh` | **New** | 2.2 |
| `.github/workflows/verify.yml` | **New** | 3.1 |
| `tests/smoke_test.rs` | **New** | 3.2 |
| `test/fixtures/insecure.py` | **New** | 3.2 |
| `AGENTS.md` | Edit | 4 |

---

## Verification Criteria

After all phases are implemented:

1. `detect_os` cognitive complexity drops from 21 to ~3 (metrics inflator fixed)
2. `praetor report` on a random repo shows no state graph noise by default
3. `semgrep` installed via `pip install` → `praetor report` includes Semgrep diagnostics
4. Datalog rules fire on real naming conventions when configured in `.praetor.toml`
5. `praetor validate --warn` exits 1 when unproven diagnostics exist
6. `praetor init` creates .praetor/ + pre-commit hook in one command
7. Pre-commit hook rejects commits with new unproven WARNING/ERROR diagnostics
8. A shadow benchmark + registry entry silences the corresponding diagnostic
9. CI workflow runs `praetor validate` on every PR
