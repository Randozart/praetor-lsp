# Setting Up Praetor

Praetor is a quadruple-bookkeeping formal verification LSP server. Once
configured, it runs on every keystroke and enforces code quality checks
before any commit lands.

---

## Prerequisites

- Rust toolchain (for building from source)
- OpenCode (recommended) or any LSP-capable editor
- (Optional) `pip install semgrep` for security linting

---

## Step 1: Build and install

```bash
cd ~/praetor-lsp
cargo install --path .
```

Or run directly from the build directory:

```bash
~/praetor-lsp/target/debug/praetor
```

---

## Step 2: Register as an LSP server

### In OpenCode

Add to `~/.config/opencode/opencode.json` or `opencode.jsonc`:

```jsonc
{
  "lsp": {
    "praetor": {
      "command": ["/path/to/praetor"],
      "extensions": [".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".c", ".h",
                     ".cpp", ".cc", ".cxx", ".hpp", ".rs", ".java"]
```

This covers **9 languages**: Python, JavaScript, TypeScript (TSX), Go, C, C++, Rust, Java.

### In VS Code, Neovim, Helix, etc.

Configure your editor to start `praetor` as a language server. Consult your
editor's LSP documentation for the specific config format.

### Verifying the LSP is active

Open a source file. If Praetor is running, diagnostics will appear in the
editor. OpenCode users can check the LSP server status within the editor.

---

## Step 3: Initialize in your project

```bash
cd /path/to/your/project
praetor init
```

This creates:
- `.praetor/` — shadow verification registry directory
- `.praetor.toml` — config file with thresholds and exempt patterns
- `.git/hooks/pre-commit` — blocks commits that introduce new diagnostics

---

## Step 4: Make the AI aware of Praetor

The AI (LLM) must know Praetor is active to respect its diagnostics.
This document plus `AGENTS.md` at the project root serve as the AI's
instruction manual.

When the AI starts work, it checks:
1. Does `~/.config/opencode/opencode.json` have a `praetor` LSP entry?
2. Does the project have `.praetor.toml`?
3. Does `AGENTS.md` mention Praetor enforcement?

If all three are present, Praetor is active and the AI must obey its
diagnostics.

---

## Verification workflow

```
Write code  →  LSP shows diagnostics  →  Refactor or shadow  →  Commit
                                    ↓                            ↓
                            Pre-commit hook                CI gate (PR)
                            runs `praetor validate`        runs same check
```

Every commit and every PR enforces the same checks. There is no bypass.

---

## Using the shadow escape hatch

If a diagnostic fires on performance-critical code that genuinely cannot
be refactored without slowing down:

1. Write a shadow function immediately after the original with a
   `// praetor-shadow: original=<name>` comment
2. Write a benchmark test in a `#[cfg(test)]` module with IO equivalence
   checks, metric comparison, and timing
3. Run: `cargo test bench_<name>`
4. The function must pass three gates: **IO equivalence**, **metric
   improvement**, and **benchmark performance parity**
5. If the original wins, the registry is updated and the warning is
   permanently silenced. If the shadow wins, it replaces the original.

See [SHADOW-VERIFICATION.md](docs/SHADOW-VERIFICATION.md) for full details.

---

## Commands reference

| Command | Description |
|---------|-------------|
| `praetor` | Start the LSP server (default) |
| `praetor init` | Set up `.praetor/` directory + pre-commit hook |
| `praetor report --target <dir>` | Full project verification report |
| `praetor validate` | CI gate — exit 1 on unproven diagnostics |
| `praetor validate --warn` | Same, but only ERROR-level causes failure |
| `praetor verify --shadow <file>` | Generate benchmark scaffold |

---

## Troubleshooting

| Problem | Likely fix |
|---------|------------|
| Praetor LSP not starting | Check the binary path in `opencode.json`. Run `which praetor`. |
| No diagnostics appear | Check the file extension is in the Praetor config. Restart the editor. |
| Pre-commit hook blocking | Read the diagnostic message. Either refactor or write a shadow. |
| `praetor init` fails | Ensure the project has a `.git` directory. |
| Shadow benchmark not running | Ensure the test is in a `#[cfg(test)]` module with `#[test]` annotation. |
