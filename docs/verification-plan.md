# OpenCode Formal Verification & Complexity Analysis Integration Plan

## Overview

This plan describes integrating formal-verification-grade static analysis and
deterministic Big-O complexity analysis into OpenCode via custom LSP servers.

## Architecture

```
OpenCode Agent
  |
  ├── Built-in LSPs (auto-enabled via "lsp": true)
  │   ├── pyright        → Python type checking
  │   ├── gopls          → Go semantic analysis
  │   ├── clangd         → C/C++ semantic analysis
  │   ├── rust-analyzer  → Rust borrow checker + Clippy
  │   ├── typescript     → TS/JS type checking
  │   ├── eslint         → JS/TS lint rules
  │   └── jdtls          → Java type checking
  │
  ├── Custom LSPs (opencode.jsonc)
  │   ├── SonarLint       → Cognitive complexity, bug patterns, code smells
  │   ├── Infer (wrapper) → Formal verification (Separation Logic)
  │   ├── Complexity      → Deterministic Big-O analysis (tree-sitter)
  │   ├── HLS + LiquidHaskell → Refinement types for Haskell
  │   └── ALS + SPARK     → Ada/SPARK formal verification
  │
  └── AGENTS.md → Instructs agent to listen but exercise judgment
```

## Tools Breakdown

### Tier 1: Built-in (already active)

| Server     | Extensions         | What it catches                    |
|------------|--------------------|------------------------------------|
| pyright    | .py, .pyi          | Type errors, undefined names       |
| gopls      | .go                | Type errors, unused vars           |
| clangd     | .c, .cpp, .h       | Type errors, missing includes      |
| rust-analyzer | .rs             | Borrow checker, Clippy lints       |
| typescript | .ts, .tsx, .js, .jsx | Type errors, semantic issues    |
| eslint     | .ts, .tsx, .js, .jsx | Lint rules, code style          |
| jdtls      | .java              | Java type checking                 |

### Tier 2: Custom (implemented here)

| Server         | Extensions                               | What it adds                                     |
|----------------|------------------------------------------|--------------------------------------------------|
| SonarLint      | .py .js .ts .go .java .cpp .c .cs .rb    | Cognitive Complexity, Cyclomatic Complexity, bug patterns |
| Infer          | .c .cpp .cc .h .hpp .java .cs .m .mm     | Separation Logic verification: memory safety, null safety, race freedom |
| Complexity     | * (all files)                            | Deterministic Big-O estimation via tree-sitter AST pattern analysis |

### Tier 3: Niche-language (install separately)

| Server         | Languages     | Install command                        |
|----------------|---------------|----------------------------------------|
| HLS + LiquidHaskell | .hs, .lhs | `ghcup install hls` (enable LH plugin) |
| ALS + SPARK    | .adb, .ads    | `sudo apt install gnat gprbuild`       |

## Configuration

File: `~/.config/opencode/opencode.jsonc`

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "lsp": {
    "sonar": {
      "command": ["sonarlint-ls", "-stdio"],
      "extensions": [".py", ".js", ".ts", ".go", ".java", ".cpp", ".c", ".cs", ".rb", ".rs"]
    },
    "infer": {
      "command": ["python3", "/home/randozart/.opencode/bin/infer_lsp.py"],
      "extensions": [".c", ".cpp", ".cc", ".h", ".hpp", ".java", ".cs", ".m", ".mm"]
    },
    "complexity": {
      "command": ["python3", "/home/randozart/.opencode/bin/complexity_lsp.py"],
      "extensions": ["*"]
    },
    "haskell": {
      "command": ["haskell-language-server-wrapper", "--lsp"],
      "extensions": [".hs", ".lhs"]
    },
    "ada": {
      "command": ["ada_language_server", "--stdio"],
      "extensions": [".adb", ".ads"]
    }
  }
}
```

## Custom LSP Implementations

### 1. Infer LSP Wrapper (`~/.opencode/bin/infer_lsp.py`)

Bridges Infer's CLI output to LSP diagnostics.

- Listens for `textDocument/didSave`
- Runs `infer --pulse-only -- <filepath>`
- Parses Infer's JSON output (bug type, line number, message)
- Publishes LSP `Diagnostic` and `PublishDiagnostics` notifications

### 2. Tree-sitter Complexity LSP (`~/.opencode/bin/complexity_lsp.py`)

Deterministic Big-O analysis via AST pattern matching.

- Listens for `textDocument/didChange` (live, as-you-type)
- Uses tree-sitter to parse the current file into an AST
- Analyzes each function body for:
  - Loop nesting depth (`O(n^k)` where k = nesting)
  - Recursion without memoization (`O(2^n)`)
  - Linear operations inside loops (`O(n*m)`)
  - Known anti-patterns
- Publishes LSP `InlayHint` at each function signature

## Installation (manual steps after config)

```bash
# SonarLint
sudo snap install sonarlint-ls --classic

# Infer (download from https://github.com/facebook/infer/releases)
# e.g. infer-linux64-v1.2.0.tar.xz
tar xf infer-linux64-v1.2.0.tar.xz
sudo ln -s $(pwd)/infer/bin/infer /usr/local/bin/infer

# Python deps for custom LSPs
pip install pygls tree-sitter

# Haskell
ghcup install ghc 9.10
ghcup install hls

# Ada/SPARK
sudo apt install gnat gprbuild
```

## AGENTS.md

A top-level `AGENTS.md` file instructs the OpenCode agent to:

1. Read LSP diagnostics from all verification servers
2. Prioritize fixes based on severity (error > warning > hint)
3. Exercise judgment: consider whether a suggested fix improves code
   quality without introducing complexity or breaking changes
4. Never blindly apply every LSP suggestion — weigh tradeoffs
5. For Big-O hints: prefer algorithmic improvements when the hint
   indicates O(n^2) or worse and a clear O(n) or O(n log n)
   alternative exists
