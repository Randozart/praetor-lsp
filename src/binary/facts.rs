/// Extract Datalog facts from a binary program model.
use crate::binary::lift::BinaryProgram;

/// A Datalog-compatible fact from binary analysis.
#[derive(Debug, Clone)]
pub enum BinaryFact {
    Function { id: u32, address: u64, name: String, size: usize },
    BasicBlock { id: u32, function_id: u32, address: u64, size: usize },
    Instruction { address: u64, mnemonic: String, operands: String },
    Call { from: u64, to: u64, target: String },
    Branch { from: u64, to: u64 },
    Alloc { address: u64, size: usize, kind: String },
}

/// Extract facts from a binary program.
pub fn extract_facts(program: &BinaryProgram) -> Vec<BinaryFact> {
    let mut facts = Vec::new();

    for (fn_id, func) in program.functions.iter().enumerate() {
        let fn_id = fn_id as u32;
        let name = func.name.clone().unwrap_or_else(|| format!("func_{:#x}", func.address));
        facts.push(BinaryFact::Function { id: fn_id, address: func.address, name, size: func.size });
        extract_block_facts(&mut facts, fn_id, func, program);
    }

    facts
}

fn extract_block_facts(facts: &mut Vec<BinaryFact>, fn_id: u32, func: &crate::binary::lift::Function, program: &BinaryProgram) {
    for block in &func.basic_blocks {
        facts.push(BinaryFact::BasicBlock {
            id: facts.len() as u32, function_id: fn_id,
            address: block.start, size: (block.end - block.start) as usize,
        });
    }
    for insn in func.basic_blocks.iter().flat_map(|b| b.instructions.iter()) {
        facts.push(BinaryFact::Instruction {
            address: insn.address, mnemonic: insn.mnemonic.clone(), operands: insn.operands.clone(),
        });
        extract_insn_facts(insn, program, facts);
    }
}

fn extract_insn_facts(insn: &crate::binary::lift::DecodedInsn, program: &BinaryProgram, facts: &mut Vec<BinaryFact>) {
    if insn.is_call {
        if let Some(target) = insn.call_target {
            let target_name = resolve_name(target, program);
            facts.push(BinaryFact::Call {
                from: insn.address, to: target, target: target_name,
            });
        }
    }

    if insn.is_branch {
        if let Some(target) = insn.branch_target {
            facts.push(BinaryFact::Branch {
                from: insn.address, to: target,
            });
        }
    }

    let mnemonic_upper = insn.mnemonic.to_uppercase();
    if mnemonic_upper.contains("ALLOC") || mnemonic_upper == "SUB" && insn.operands.contains("rsp") {
        if let Some(size) = extract_sub_rsp_size(&insn.operands) {
            facts.push(BinaryFact::Alloc {
                address: insn.address, size, kind: "stack".into(),
            });
        }
    }
}

/// Resolve a function name from an address.
fn resolve_name(address: u64, program: &BinaryProgram) -> String {
    if let Some(idx) = program.fn_map.get(&address) {
        if let Some(name) = &program.functions[*idx].name {
            return name.clone();
        }
    }
    format!("sub_{:#x}", address)
}

/// Extract allocation size from `sub rsp, <imm>`.
fn extract_sub_rsp_size(operands: &str) -> Option<usize> {
    let operands = operands.trim();
    if operands.starts_with("rsp, 0x") || operands.starts_with("rsp, 0X") {
        let hex_str = operands.split_once(", ").and_then(|(_, s)| s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")))?;
        usize::from_str_radix(hex_str, 16).ok()
    } else if operands.starts_with("rsp, ") {
        operands.split_once(", ").and_then(|(_, s)| s.parse::<usize>().ok())
    } else {
        None
    }
}