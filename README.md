# Praetor

**Quadruple-bookkeeping formal verification LSP.**

Praetor is a language server that enforces four mutually-reinforcing pillars of
verification: **Code**, **Docs**, **State Graph**, and **Datalog Facts**. It
combines tree-sitter AST analysis, a built-in Datalog engine, and integration
with industry-standard security tools to provide real-time verification in any
LSP-capable editor.

> **Phase of development:** Early but functional. Praetor self-verifies (it runs
> on its own source code) and produces diagnostics on any supported language.
> External tool bridges (Semgrep, Infer) are integrated but require the
> corresponding CLI tools to be installed separately. See [docs/ENFORCEMENT-PLAN.md](./docs/ENFORCEMENT-PLAN.md)
> for the full roadmap.

---

## Features

- **9 languages** — Python, JavaScript, TypeScript, TSX, Go, C, C++, Rust, Java
- **14 tree-sitter parsers** — 9 languages × grammar variants (JS/JSX, TS/TSX, C/C++, etc.)
- **Big-O complexity analysis** — loop nesting depth, recursion detection, linear ops inside loops
- **Strict Intent Mode** — doc comment enforcement before every function
- **Datalog invariant engine** — 5 built-in rules (privacy, reachability, parameter limits, data leaks) powered by [crepe](https://crates.io/crates/crepe)
- **Cyclomatic & cognitive complexity** — configurable thresholds
- **Architecture heuristics** — god object, data class, deep inheritance detection
- **State graph validation** — declare allowed state transitions in `.praetor/state-graph.json`
- **Semgrep integration** — 2,000+ security rules for all 9 languages
- **Infer integration** — Facebook/Meta's static analyzer for C/C++/Java
- **SonarLint integration** — (stub, requires JAR subprocess)
- **Project report** — Markdown or HTML, per-file diagnostic breakdown
- **Shadow verification** — benchmark-gated refactoring: prove a refactor doesn't regress performance, or keep the original
- **Pre-commit hook** — automatic via `praetor init`, blocks commits with unproven diagnostics
- **CI gate** — `praetor validate --warn --json` for GitHub Actions

---

## Quick start

```bash
# Install with cargo
cargo install --path .

# Initialize in your project
praetor init

# Run a verification report
praetor report --target .

# Run the LSP server (used by editors)
praetor
```

### OpenCode integration

Praetor is registered as an LSP server in OpenCode. Add to your
`~/.config/opencode/opencode.json`:

```jsonc
{
  "lsp": {
    "praetor": {
      "command": ["/path/to/praetor"],
      "extensions": [".py", ".js", ".jsx", ".ts", ".tsx", ".go", ".c", ".h", ".cpp", ".cc", ".cxx", ".hpp", ".rs", ".java"]
    }
  }
}
```

### VS Code / Neovim / Helix / any LSP client

Configure your editor to start `praetor` as a language server. Example for
Neovim's `nvim-lspconfig`:

```lua
require('lspconfig').praetor = {
  cmd = { "praetor" },
  filetypes = { "python", "javascript", "typescript", "go", "c", "cpp", "rust", "java" },
}
```

---

## Configuration

Praetor discovers `.praetor.toml` by walking up from the current directory.
Generate a default config with:

```bash
praetor init
```

### Example `.praetor.toml`

```toml
[intent]
enabled = true
severity = "hint"
exempt_patterns = ["^get_.*", "^set_.*", "^new$", "^main$", "^test_.*", "^default$", "^into$", "^from$"]

[complexity]
big_o_threshold = "O(n²)"
cyclomatic_max = 15
cognitive_max = 15
max_function_lines = 100
max_nesting_depth = 6
max_params = 6

[state_graph]
enabled = false
path = ".praetor/state-graph.json"

[datalog]
auth_functions = ["authenticate", "authorize", "login"]
private_data_labels = ["private", "secret", "password", "token"]
entry_points = ["main", "run", "start", "handle"]
log_functions = ["log", "log_access", "audit"]
```

---

## AI integration

Praetor is designed to work with AI coding assistants. The project includes
two key documents for configuring an AI agent:

- **`AGENTS.md`** (project root or `~/.config/opencode/`) — enforcement rules,
  the three-gate verification path, and what the AI can and cannot do.
- **[docs/SETTING-UP.md](./docs/SETTING-UP.md)** — step-by-step LSP setup,
  project initialization, and troubleshooting.

For full details of the development process, see [docs/LESSONS.md](./docs/LESSONS.md)
and [docs/ENFORCEMENT-PLAN.md](./docs/ENFORCEMENT-PLAN.md).

---

## Shadow verification

When a diagnostic fires on performance-critical code that genuinely needs to
stay complex, write a shadow function and benchmark it:

```rust
// praetor-shadow: original=hot_function
fn hot_function_v2(...) { ... }  // refactored version
```

Then run:

```bash
cargo test bench_hot_function
```

If the original is faster, the warning is permanently silenced and recorded
in `.praetor/shadow-results.json`. The benchmark machine is the sole judge —
no inline exceptions, no human override.

---

## Commands

| Command | Description |
|---------|-------------|
| `praetor` | Start the LSP server |
| `praetor report --target <dir>` | Generate verification report |
| `praetor validate --warn` | CI gate (exit 1 on unproven diagnostics) |
| `praetor init` | Set up `.praetor/` and pre-commit hook |
| `praetor verify --shadow <file>` | Generate benchmark scaffold |

---

## Architecture

Praetor enforces **four pillars** of verification:

1. **Code** – The implementation. Metrics, complexity, architecture heuristics.
2. **Docs** – Intent comments. Every function must declare its expected behaviour.
3. **State Graph** – Declared state transitions. Code must not perform
   transitions outside the declared graph (opt-in, default disabled).
4. **Datalog Facts** – Invariants extracted from the AST and checked against
   rules shipped with Praetor. Mathematical, not probabilistic.

For full details, see [docs/QUADRUPLE-BOOKKEEPING.md](./docs/QUADRUPLE-BOOKKEEPING.md)
and [docs/SHADOW-VERIFICATION.md](./docs/SHADOW-VERIFICATION.md).

---

## External tool bridges

Praetor integrates with three industry-standard analysis tools. Each is
detected at runtime — install the relevant tool and Praetor will use it.

| Tool | Language(s) | How to install | Integration |
|------|-------------|----------------|-------------|
| [Semgrep](https://semgrep.dev) | All 9 | `pip install semgrep` | Structured JSON output parsed into diagnostics |
| [Infer](https://fbinfer.com) | C, C++, Java | `brew install infer` or download from GitHub | Runs infer, reads `infer-out/report.json` |
| [SonarLint](https://sonarsource.com) | All 9 | `java -jar sonarlint-language-server.jar` (LSP subprocess, stub) | LSP-to-LSP bridge (not yet implemented) |

---

## Credits and acknowledgements

### Tree-sitter grammars

Praetor's AST analysis is built on [tree-sitter](https://tree-sitter.github.io).
We use the following official grammars:

| Grammar | Language | Author | License |
|---------|----------|--------|---------|
| [tree-sitter-python](https://crates.io/crates/tree-sitter-python) | Python | Max Brunsfeld, Ayman Nadeem | MIT |
| [tree-sitter-javascript](https://crates.io/crates/tree-sitter-javascript) | JavaScript (JSX) | Max Brunsfeld | MIT |
| [tree-sitter-typescript](https://crates.io/crates/tree-sitter-typescript) | TypeScript (TSX) | Max Brunsfeld | MIT |
| [tree-sitter-go](https://crates.io/crates/tree-sitter-go) | Go | Max Brunsfeld | MIT |
| [tree-sitter-c](https://crates.io/crates/tree-sitter-c) | C | Max Brunsfeld | MIT |
| [tree-sitter-cpp](https://crates.io/crates/tree-sitter-cpp) | C++ | Max Brunsfeld | MIT |
| [tree-sitter-rust](https://crates.io/crates/tree-sitter-rust) | Rust | Max Brunsfeld | MIT |
| [tree-sitter-java](https://crates.io/crates/tree-sitter-java) | Java | Max Brunsfeld | MIT |

### Key dependencies

| Crate | Author(s) | Purpose | License |
|-------|-----------|---------|---------|
| [tower-lsp](https://crates.io/crates/tower-lsp) | Eduard-Mihai Burtescu, Tessel | LSP framework | MIT / Apache-2.0 |
| [crepe](https://crates.io/crates/crepe) | Łukasz Niemier | Datalog engine | MIT / Apache-2.0 |
| [tree-sitter](https://crates.io/crates/tree-sitter) | Max Brunsfeld | AST parsing | MIT |
| [clap](https://crates.io/crates/clap) | Kevin K., Ed Page, et al. | CLI argument parsing | MIT / Apache-2.0 |
| [serde](https://crates.io/crates/serde) | Erick Tryzelaar, David Tolnay | Serialization | MIT / Apache-2.0 |
| [serde_json](https://crates.io/crates/serde_json) | Erick Tryzelaar, David Tolnay | JSON parsing | MIT / Apache-2.0 |
| [toml](https://crates.io/crates/toml) | Alex Crichton | Config parsing | MIT / Apache-2.0 |
| [tokio](https://crates.io/crates/tokio) | Tokio Contributors | Async runtime | MIT |
| [regex](https://crates.io/crates/regex) | Andrew Gallant | Regex engine | MIT / Apache-2.0 |
| [walkdir](https://crates.io/crates/walkdir) | Andrew Gallant | Directory walking | MIT |
| [sha2](https://crates.io/crates/sha2) | RustCrypto | SHA-256 hashing | MIT / Apache-2.0 |

### External tools

| Tool | Author | Purpose | License |
|------|--------|---------|---------|
| [Semgrep](https://semgrep.dev) | r2c / Semgrep Inc. | Security linting | LGPL-2.1 |
| [Infer](https://fbinfer.com) | Facebook / Meta | Static analysis | MIT |
| [SonarLint](https://sonarsource.com/products/sonarlint) | SonarSource | Code quality | LGPL-3.0 |

### Additional resources

The Datalog rules are inspired by formal verification patterns from
[Crepe's examples](https://github.com/ekirder/crepe) and the
[Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
specification by Microsoft.

The quadruple-bookkeeping architecture draws on concepts from
[Design by Contract](https://en.wikipedia.org/wiki/Design_by_contract)
(Bertrand Meyer) and
[Model Checking](https://en.wikipedia.org/wiki/Model_checking)
(Clarke, Emerson, Sifakis).

---

## License

Praetor is distributed under the MIT license. See [LICENSE](./LICENSE) for
details.

External tools carry their own licenses as listed above.
