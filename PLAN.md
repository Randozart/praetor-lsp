# Praetor LSP — Universal Verification & Complexity Package for OpenCode

## Vision

A single Rust binary (`praetor-lsp`) that provides language-agnostic formal
verification, complexity analysis, security scanning, and engineering best-practice
enforcement to any LSP-capable client (OpenCode primary target).

The name **Praetor** — a Roman magistrate responsible for command of justice —
reflects the tool's role: it judges code quality, enforces standards, and ensures
every function accounts for its own intent before being accepted.

## Architecture

```
OpenCode / any LSP client
         │
         │  LSP (stdio)
         ▼
┌────────────────────────────────────────────┐
│            praetor-lsp                      │
│  (single Rust binary, tower-lsp)            │
│                                             │
│  ┌─────────────────────────────────────┐   │
│  │  Tree-sitter AST Engine             │   │
│  │  • 30+ compiled grammars            │   │
│  │  • Per-keystroke re-parse           │   │
│  │  • Language-agnostic queries        │   │
│  └──────────┬──────────────────────────┘   │
│             │                                │
│  ┌──────────▼──────────────────────────┐   │
│  │  Check Pipeline                     │   │
│  │                                     │   │
│  │  1. Intent Documentation Check      │   │
│  │     (comment/docstring required)    │   │
│  │                                     │   │
│  │  2. Security / Idiomatic (Semgrep)  │   │
│  │     (child process, auto-managed)   │   │
│  │                                     │   │
│  │  3. Complexity Analysis (native)    │   │
│  │     Big-O, cyclomatic, nest depth   │   │
│  │                                     │   │
│  │  4. Architectural/SOLID Checks      │   │
│  │     (heuristic AST patterns)        │   │
│  │                                     │   │
│  │  5. Formal Verification Bridge      │   │
│  │     Infer / SonarLint (auto-disk)   │   │
│  └─────────────────────────────────────┘   │
└────────────────────────────────────────────┘
```

## Five Check Categories

### 1. Strict Intent Mode (native — always enforced)

Every function, method, class, trait, and interface MUST be preceded by a
documentation comment or docstring explaining its purpose and expected behaviour.

| Language | Required annotation |
|----------|-------------------|
| Python | Docstring (`"""..."""`) as first statement in function body |
| Rust | `///` or `//!` doc comment immediately preceding the item |
| TypeScript/JS | JSDoc `/** ... */` comment before declaration |
| Go | `//` comment before function |
| C/C++/Java | `/* ... */` or `///` comment before declaration |

**Cross-reference check (optional):** If the comment says `Expected behavior:
Input must be > 0`, the LSP checks the function body for a guard clause
(`if x <= 0 { return error }`). Missing guard → diagnostic.

### 2. Complexity & Metrics (native — tree-sitter AST walk)

| Metric | Detection method | Example output |
|--------|-----------------|----------------|
| Loop nesting depth | AST walk counting loop nodes | `O(n²) — depth 2` |
| Recursion | Self-call detection in function body | `O(2ⁿ) — recursive` |
| Linear ops in loops | `.indexOf()`, `.find()`, etc. inside loop | `O(n·m) — linear ops in loop` |
| Cyclomatic complexity | Decision-point counting (if, else, case, &&, \|\|) | `CC: 12 (threshold: 10)` |
| Function length | AST node span in lines | `fn foo — 47 lines (threshold: 50)` |
| Cognitive complexity | Nesting depth + boolean logic weight | `CogC: 24 (threshold: 15)` |
| Parameter count | Function definition children | `fn bar — 8 params (threshold: 6)` |

### 3. Security & Idiomatic Patterns (Semgrep child process)

- Auto-downloads Semgrep OSS binary on first run (`~/.praetor-lsp/bin/semgrep`)
- Bundled rule packs: OWASP Top 10, framework best practices
- Runs on file save, parses `--json` output to LSP diagnostics

| What it catches | Example message |
|----------------|----------------|
| Hardcoded secrets | `Hardcoded API key detected` |
| SQL injection | `Raw SQL concatenation — use parameterized query` |
| Unhandled promise rejections | `Promise without .catch() — floating rejection` |
| React hook in conditional | `React hook called inside conditional — violates Rules of Hooks` |
| Unsafe deserialization | `Unvalidated user input passed to eval/exec` |

### 4. Architectural / SOLID Heuristics (native — AST pattern matching)

- **God Object detection**: class with > 300 lines or > 10 public methods
- **Feature envy**: method that calls more getters/setters on another class than its own
- **Shotgun surgery**: function that modifies > 3 different data structures
- **Divergent change**: class changed for multiple different reasons (heuristic via field count + method diversity)

### 5. Formal Verification Bridge (auto-discover external tools)

| Tool | Languages | Detection method |
|------|-----------|-----------------|
| Infer | C/C++/Java/C#/Obj-C | `$PATH` or `~/.praetor-lsp/bin/infer` |
| SonarLint | 30+ langs | `~/.praetor-lsp/lib/sonarlint-language-server.jar` |
| Prusti | Rust | `$PATH` — `cargo prusti` |
| Clippy | Rust | Built into rust-analyzer (no bridge needed) |

Output is normalised to Praetor's diagnostic format.

## Configuration

### OpenCode config (`opencode.json`)

Single entry:

```jsonc
{
  "lsp": {
    "praetor": {
      "command": ["praetor-lsp"],
      "extensions": ["*"]
    }
  }
}
```

### Project config (`.praetor.toml` — auto-discovered from project root)

```toml
[intent]
enabled = true
severity = "error"          # error | warning | hint
exempt_patterns = [
  "fn get_.*",              # getters
  "fn set_.*",              # setters
  "fn new\\(.*",            # constructors
  "fn main\\(.*",           # entry points
  "fn test_.*",             # tests
  "#\\[derive\\(.*\\)\\]",  # Rust derive macros
]

[complexity]
big_o_threshold = "O(n²)"   # Functions at this level or higher get tagged
cyclomatic_max = 15
cognitive_max = 15
max_function_lines = 50
max_nesting_depth = 4
max_params = 6

[security]
enabled = true
ruleset = "bundled"         # "bundled" | path to custom rules
extra_rules_dir = ".praetor-rules"

[architecture]
god_class_lines = 300
god_class_methods = 10
max_dependencies = 8

[formal_verification]
auto_discover = true
disable = ["sonarlint"]     # Opt-out specific tools

[lsp]
extensions = ["*"]
exclude_extensions = [".md", ".txt", ".json", ".yaml", ".toml"]
```

## Auto-Download Manager

On first run, Praetor creates `~/.praetor-lsp/` and downloads:

| Component | Size | Source | Required? |
|-----------|------|--------|-----------|
| Semgrep binary | ~30 MB | GitHub releases | Optional (security checks) |
| SonarLint JAR | ~52 MB | Maven Central | Optional (broad language coverage) |
| Infer (Linux only) | ~200 MB | GitHub releases | Optional (C/C++/Java formal verification) |

All downloads are cached and version-checked. If a component is missing,
the corresponding checks are silently skipped with a single info-level log line.

## Dependency Handling

| Language | Built-in LSP (auto-starts) | Praetor native checks | External tool bridge |
|----------|---------------------------|----------------------|---------------------|
| Python | pyright | Intent, Complexity, Architecture | Semgrep |
| TypeScript/JS | typescript-language-server | Intent, Complexity, Architecture | Semgrep, SonarLint |
| Go | gopls | Intent, Complexity, Architecture | Semgrep |
| C/C++ | clangd | Intent, Complexity, Architecture | Infer, SonarLint |
| Rust | rust-analyzer | Intent, Complexity, Architecture | Prusti (optional) |
| Java | jdtls | Intent, Complexity, Architecture | Infer, SonarLint |
| C# | csharp (built-in) | Intent, Complexity, Architecture | Infer, SonarLint |
| Ruby | — | Intent, Complexity, Architecture | Semgrep, SonarLint |
| Haskell | haskell-language-server | Intent, Complexity | LiquidHaskell (via HLS) |
| Ada | ada_language_server | Intent, Complexity | SPARK (via ALS) |

## Project Structure

```
praetor-lsp/
├── Cargo.toml
├── build.rs                      # Compiles tree-sitter grammars into binary
├── src/
│   ├── main.rs                   # Entry point, LSP server init
│   ├── lsp.rs                    # tower-lsp handler trait impls
│   ├── ast/
│   │   ├── mod.rs                # AST engine: parsing + tree-sitter abstraction
│   │   ├── languages.rs          # Language → node type mappings
│   │   └── query.rs              # Universal S-expression query patterns
│   ├── checks/
│   │   ├── mod.rs
│   │   ├── intent.rs             # Strict Intent Mode: comment/docstring detection
│   │   ├── complexity.rs         # Big-O, loop depth, recursion, linear ops
│   │   ├── metrics.rs            # Cyclomatic, cognitive, line count, params
│   │   └── architecture.rs       # SOLID heuristics: god objects, coupling
│   ├── bridge/
│   │   ├── mod.rs
│   │   ├── semgrep.rs            # Semgrep process manager + JSON parser
│   │   ├── infer.rs              # Infer output parser (infer-out/report.json)
│   │   └── sonarlint.rs          # SonarLint JAR process manager
│   ├── config.rs                 # .praetor.toml parser (serde)
│   └── downloader.rs             # Auto-download + cache manager
├── rules/                        # Bundled Semgrep rule packs
│   ├── python/
│   ├── javascript/
│   ├── go/
│   └── ...
├── grammars/                     # tree-sitter grammar submodules
├── scripts/                      # Python prototype LSPs (reference)
│   ├── infer_lsp.py
│   └── complexity_lsp.py
├── lib/                          # Downloaded external binaries
│   └── sonarlint-language-server.jar
├── config/
│   └── opencode.jsonc.example    # Reference OpenCode config
├── docs/
│   └── verification-plan.md      # Original integration plan
├── PLAN.md                       # This file
└── .gitignore
```

## Development Phases

### Phase 1: Rust Skeleton + Core AST Engine (target: 1 week)
- `cargo init` with `tower-lsp`, `tree-sitter`, `serde`, `toml` dependencies
- LSP server that responds to `initialize`, `shutdown`, `textDocument/didChange`
- tree-sitter parser for 3 languages: Python, TypeScript, Go
- `InlayHint` provider returning placeholder complexity annotations

### Phase 2: Complexity Checks (target: 1 week)
- Port complexity logic from `scripts/complexity_lsp.py` to Rust
- Loop depth, recursion detection, linear-op detection
- Cyclomatic complexity counting (decision points)
- Function length and parameter count metrics
- Expand language support to 8+ languages

### Phase 3: Strict Intent Mode (target: 1 week)
- Comment node detection before function/class declarations
- Docstring parsing for Python (`"""..."""`) and Rust (`///`)
- JSDoc detection for TypeScript/JS (`/** ... */`)
- `.praetor.toml` config file parsing
- Exempt pattern matching
- Cross-reference check (optional): guard clause matching declared preconditions

### Phase 4: Auto-Download Manager (target: 1 week)
- Semgrep binary download, verification, caching
- Semgrep rule runs + JSON output parser
- First-run setup: `~/.praetor-lsp/` directory creation
- Version checking for cached binaries

### Phase 5: Formal Verification Bridge (target: 1 week)
- Infer `report.json` parser
- SonarLint JAR process lifecycle management
- Unified diagnostic format across all bridge tools
- Architecture/SOLID heuristics

### Phase 6: Polish + Distribution (target: 1 week)
- GitHub Actions: `cross` for Linux/macOS/Windows x86_64 + aarch64
- Install script (`install.sh` / `install.ps1`)
- Homebrew tap (`brew install praetor-lsp`)
- Comprehensive README with usage examples
- Documentation site or README section for each check

## Relationship to Python Prototype Scripts

The Python scripts (`scripts/infer_lsp.py`, `scripts/complexity_lsp.py`) are
functional prototypes that prove the LSP integration patterns work. They coexist
with the Rust binary during development. Once the Rust binary reaches feature
parity, they become deprecated reference material.

## Distribution

### Release artifacts

```
praetor-lsp-x86_64-linux.tar.gz
praetor-lsp-x86_64-macos.tar.gz
praetor-lsp-aarch64-macos.tar.gz
praetor-lsp-x86_64-windows.zip
```

Each contains: `praetor-lsp` binary (statically linked, ~15-20 MB) + bundled rules.

### Install commands

```bash
# Linux / macOS
curl -fsSL https://github.com/yourname/praetor-lsp/releases/latest/download/install.sh | sh

# macOS (future)
brew install yourname/tap/praetor-lsp

# From source (future)
cargo install praetor-lsp
```

## Why Rust

| Factor | Rust | Go | Python |
|--------|------|----|--------|
| LSP library | `tower-lsp` (mature, async) | `go.lsp.dev` (stable) | `pygls` (functional) |
| tree-sitter | Native `tree-sitter` crate, compiled grammars | `go-tree-sitter` (CGo overhead) | FFI-based |
| Single binary | ✅ Static link, ~15 MB | ✅ Static link, ~10 MB | ❌ Requires Python runtime |
| Cross-compile | ✅ `cross` / `zigbuild` | ✅ Native | N/A |
| Bundle size | 15-20 MB with grammars | 10-15 MB | N/A |
| Community LSP tooling | Mature ecosystem | Growing | Limited |

Rust wins because tree-sitter grammars compile natively without CGo overhead,
`tower-lsp` is the most robust LSP framework, and the single-binary distribution
model aligns with the "no dependency hell" vision.
