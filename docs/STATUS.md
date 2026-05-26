# Praetor — Status Record

> Generated: 2026-05-26T10:05:55Z

---

## Phase 1 — Language Expansion (Complete ✅)

**2026-05-26T09:25:00Z** — 8 new languages added, 20 total, 33 extensions:

| Language | Extensions | Status |
|----------|-----------|--------|
| Python | `.py` | ✅ |
| JavaScript | `.js`, `.jsx` | ✅ |
| TypeScript | `.ts` | ✅ |
| TSX | `.tsx` | ✅ |
| Go | `.go` | ✅ |
| C | `.c`, `.h` | ✅ |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` | ✅ |
| Rust | `.rs` | ✅ |
| Java | `.java` | ✅ |
| Assembly | `.asm`, `.s`, `.S`, `.assembly` | ✅ |
| SystemVerilog | `.sv`, `.svh` | ✅ |
| VHDL | `.vhd`, `.vhdl` | ✅ |
| Ruby | `.rb` | ✅ |
| Lua | `.lua` | ✅ |
| PHP | `.php` | ✅ |
| Swift | `.swift` | ✅ |
| Zig | `.zig` | ✅ |
| Dart | `.dart` | ✅ |
| Perl | `.pl`, `.pm` | ✅ |
| Haskell | `.hs`, `.lhs` | ✅ |
| **Kotlin** | `.kt`, `.kts` | ❌ tree-sitter-kotlin v0.3.x needs tree-sitter <0.23, C symbols conflict with 0.26 |
| **COBOL** | `.cbl`, `.cob` | ❌ no Rust lib target on crates.io |

Files changed: `Cargo.toml` (9 deps + tree-sitter 0.25→0.26), `languages.rs` (+8 configs + 14 extensions), `setup.rs` (+9 pip packages), `opencode.jsonc` (+14 extensions)

---

## Phase 2A — rizin LSP Bridge (Complete ✅)

**2026-05-26T09:35:00Z** — Binary analysis via rizin:

- `scripts/rizin_lsp.py`: Python LSP wrapping r2pipe
  - Hover: disassembly at cursor
  - Goto-def: navigate to function definition
  - References: cross-references
  - Document symbols: function listing
- rizin v0.8.2 static build auto-downloaded via `praetor setup`
- Registered in OpenCode config for `.dll`, `.exe`, `.so`, `.o`, `.bin`, `.elf`, `.sys`

---

## Phase 2B — Native Binary Analysis (Complete ✅)

**2026-05-26T10:00:00Z** — Zero-dependency binary analysis via `goblin` + `iced-x86`:

- `src/binary/lift.rs`: PE/ELF/Mach-O parsing, disassembly, basic block extraction
- `src/binary/facts.rs`: Datalog-compatible facts (functions, blocks, calls, branches, stack allocs)
- `src/binary/patterns.rs`: Anti-pattern detection
  - Spin-locks (tight loop + test/cmp, no calls)
  - Polling loops (loop + memory read)
  - Busy-wait (pause + backward jmp)
  - Memory bloat (>1MB stack alloc)
  - Legacy API calls (gethostbyname, socket, etc.)
- `--binary` flag on `praetor report --target DIR --binary`

Verified: analyzed 2406 `.so` files, detected real anti-patterns in libfdt.

---

## Phase 3 — Surgical Patching (Complete ✅)

**2026-05-26T10:45:00Z** — Byte-level surgery + CFG topology verification:

- `src/binary/patch.rs`: Byte-level patching engine
  - `Patch::nop(addr, size)` — NOP sled generation (0x90 fill)
  - `Patch::near_jump(from, to, is_64)` — jump redirect (short/near/absolute)
  - `Patch::near_call(from, to, is_64)` — call redirect
  - `Patch::shim(addr, stub, name)` — shim injection stub
  - `apply_patches(data, patches, image_base)` — apply with overlap detection
  - `nop_out_call(data, addr, is_64)` — surgically NOP out a call instruction
- `src/binary/verify.rs`: CFG topology equivalence checker
  - `compare_binaries(orig_path, patched_path)` — full structural diff
  - Reports: matched/modified/added/removed functions, preserved/new/removed call edges
  - `format_topology_report(report)` — human-readable diff output
- CLI: `praetor binary verify --original a.so --patched b.so`
- CLI: `praetor binary apply --input a.so --output b.so --nop 0x42b3`

Verified: NOP'd `fdt_node_offset_by_compatible` in libfdt, CFG confirmed 100% edge preservation.

---

## Key Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI: lsp/report/verify/init/setup/validate/binary |
| `src/binary/lift.rs` | Binary loader + disassembler |
| `src/binary/facts.rs` | Datalog fact extraction |
| `src/binary/patterns.rs` | Anti-pattern detection |
| `src/binary/patch.rs` | Byte-level patching engine |
| `src/binary/verify.rs` | CFG topology equivalence checker |
| `src/binary/mod.rs` | Module declarations |
| `scripts/rizin_lsp.py` | rizin LSP bridge |
| `src/ast/languages.rs` | 20 language configs |
| `~/.config/opencode/opencode.jsonc` | LSP registrations |
