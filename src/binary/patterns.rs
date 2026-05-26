/// Anti-pattern detection in binary programs: spin-locks, polling loops, bloat, legacy calls.
use crate::binary::lift::{BinaryProgram, DecodedInsn, Function};

/// A detected anti-pattern with location and description.
#[derive(Debug, Clone)]
pub struct AntiPattern {
    pub kind: PatternKind,
    pub address: u64,
    pub function: String,
    pub description: String,
    pub severity: PatternSeverity,
}

/// Kind of anti-pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PatternKind {
    SpinLock,
    PollingLoop,
    MemoryBloat,
    LegacyCallback,
    BusyWait,
}

/// Severity of the detected pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PatternSeverity {
    Error,
    Warning,
    Info,
}

/// Detect anti-patterns in a binary program.
pub fn detect_patterns(program: &BinaryProgram) -> Vec<AntiPattern> {
    let mut patterns = Vec::new();

    for func in &program.functions {
        let fn_name = func.name.clone().unwrap_or_else(|| format!("func_{:#x}", func.address));

        // Check for spin-locks and polling loops
        if let Some(pattern) = check_spin_lock(func, &fn_name) {
            patterns.push(pattern);
        }
        if let Some(pattern) = check_polling_loop(func, &fn_name) {
            patterns.push(pattern);
        }

        // Check for busy-wait loops
        if let Some(pattern) = check_busy_wait(func, &fn_name) {
            patterns.push(pattern);
        }

        // Check for memory bloat
        for insn in &func.instructions {
            check_memory_bloat(insn, &fn_name, &mut patterns);
            check_legacy_callback(insn, &fn_name, &mut patterns);
        }
    }

    patterns
}

/// Detect spin-locks: tight loops with `test`/`cmp` + `jne` with no calls.
fn check_spin_lock(func: &Function, fn_name: &str) -> Option<AntiPattern> {
    let blocks = &func.basic_blocks;
    if blocks.len() < 2 {
        return None;
    }

    // Look for a backward branch (loop) with only test/cmp and jcc in the loop body
    for block in blocks {
        let has_backward_branch = block.instructions.iter().any(|i| {
            i.is_branch && i.branch_target.map_or(false, |t| t < block.start)
        });
        if !has_backward_branch {
            continue;
        }
        // Check if the block has no calls
        let has_calls = block.instructions.iter().any(|i| i.is_call);
        if has_calls {
            continue;
        }
        // Check for test/cmp instructions
        let has_test_or_cmp = block.instructions.iter().any(|i| {
            let m = i.mnemonic.to_uppercase();
            m.contains("TEST") || m.contains("CMP") || m.contains("LOCK")
        });
        if has_test_or_cmp {
            return Some(AntiPattern {
                kind: PatternKind::SpinLock,
                address: block.start,
                function: fn_name.into(),
                description: format!("Possible spin-lock at {:#x} — tight loop with test/cmp, no calls", block.start),
                severity: PatternSeverity::Warning,
            });
        }
    }
    None
}

/// Detect polling loops: loops checking a memory address (hardware register).
fn check_polling_loop(func: &Function, fn_name: &str) -> Option<AntiPattern> {
    let blocks = &func.basic_blocks;
    if blocks.len() < 2 {
        return None;
    }

    for block in blocks {
        let has_backward_branch = block.instructions.iter().any(|i| {
            i.is_branch && i.branch_target.map_or(false, |t| t < block.start)
        });
        if !has_backward_branch {
            continue;
        }
        // Check for memory operand reads (potential hardware register polling)
        let has_memory_read = block.instructions.iter().any(|i| {
            i.operands.contains('[') && i.operands.contains(']')
        });
        if has_memory_read {
            return Some(AntiPattern {
                kind: PatternKind::PollingLoop,
                address: block.start,
                function: fn_name.into(),
                description: format!("Possible polling loop at {:#x} — loop reading from memory address", block.start),
                severity: PatternSeverity::Warning,
            });
        }
    }
    None
}

/// Detect busy-wait loops: `pause` + `jmp` back.
fn check_busy_wait(func: &Function, fn_name: &str) -> Option<AntiPattern> {
    let has_pause = func.instructions.iter().any(|i| {
        i.mnemonic.to_uppercase().contains("PAUSE")
    });
    let has_backward_jmp = func.instructions.iter().any(|i| {
        i.is_branch && i.branch_target.map_or(false, |t| {
            t < i.address && t >= func.address
        })
    });

    if has_pause && has_backward_jmp {
        let addr = func.instructions.iter()
            .find(|i| i.mnemonic.to_uppercase().contains("PAUSE"))
            .map(|i| i.address)
            .unwrap_or(func.address);
        return Some(AntiPattern {
            kind: PatternKind::BusyWait,
            address: addr,
            function: fn_name.into(),
            description: format!("Busy-wait loop at {:#x} — PAUSE + backward jump (spin-loop hint)", addr),
            severity: PatternSeverity::Warning,
        });
    }
    None
}

/// Detect memory bloat: large stack allocations.
fn check_memory_bloat(insn: &DecodedInsn, fn_name: &str, patterns: &mut Vec<AntiPattern>) {
    let m = insn.mnemonic.to_uppercase();
    if m == "SUB" && insn.operands.to_uppercase().starts_with("RSP,") {
        // Extract allocation size
        let size = extract_allocation_size(&insn.operands);
        if let Some(size) = size {
            if size > 1024 * 1024 {
                // > 1MB on stack
                patterns.push(AntiPattern {
                    kind: PatternKind::MemoryBloat,
                    address: insn.address,
                    function: fn_name.into(),
                    description: format!("Large stack allocation at {:#x}: {} bytes", insn.address, size),
                    severity: PatternSeverity::Warning,
                });
            }
        }
    }
}

/// Detect calls to known legacy/deprecated APIs.
fn check_legacy_callback(insn: &DecodedInsn, fn_name: &str, patterns: &mut Vec<AntiPattern>) {
    if !insn.is_call {
        return;
    }
    let operands_upper = insn.operands.to_uppercase();
    let legacy_apis = [
        "GETHOSTBYNAME", "INET_ADDR", "SEND", "RECV",
        "SOCKET", "BIND", "CONNECT", "LISTEN", "ACCEPT",
        "SELECT", "POLL", "WAITFORMULTIPLEOBJECTS",
        "CREATETHREAD", "CREATEPROCESS",
        "VIRTUALALLOC", "HEAPALLOC", "GLOBALALLOC",
        "LOADLIBRARY", "GETPROCADDRESS",
        "REGOPENKEY", "REGQUERYVALUEEX",
        "CREATEFILE", "READFILE", "WRITEFILE",
    ];
    for api in &legacy_apis {
        if operands_upper.contains(api) {
            patterns.push(AntiPattern {
                kind: PatternKind::LegacyCallback,
                address: insn.address,
                function: fn_name.into(),
                description: format!("Call to legacy API {} at {:#x} — may need shimming", api, insn.address),
                severity: PatternSeverity::Info,
            });
            break;
        }
    }
}

fn extract_allocation_size(operands: &str) -> Option<usize> {
    let operands = operands.trim();
    if let Some(rest) = operands.strip_prefix("rsp, ").or_else(|| operands.strip_prefix("RSP, ")) {
        let rest = rest.trim();
        if rest.starts_with("0x") || rest.starts_with("0X") {
            usize::from_str_radix(&rest[2..], 16).ok()
        } else {
            rest.parse::<usize>().ok()
        }
    } else {
        None
    }
}