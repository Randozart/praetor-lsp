/// Binary loader and disassembler: lift PE/ELF/Mach-O to an analyzed program model.
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use goblin::elf::Elf;
use goblin::mach::Mach;
use goblin::pe::PE;
use goblin::Object;
use iced_x86::{Decoder, DecoderOptions, FlowControl};

/// A discovered function in the binary.
#[derive(Debug, Clone)]
pub struct Function {
    pub address: u64,
    pub size: usize,
    pub name: Option<String>,
    pub instructions: Vec<DecodedInsn>,
    pub basic_blocks: Vec<BasicBlock>,
}

/// A decoded instruction.
#[derive(Debug, Clone)]
pub struct DecodedInsn {
    pub address: u64,
    pub size: usize,
    pub mnemonic: String,
    pub operands: String,
    pub is_call: bool,
    pub is_branch: bool,
    pub is_return: bool,
    pub call_target: Option<u64>,
    pub branch_target: Option<u64>,
}

/// A basic block (straight-line code segment).
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub start: u64,
    pub end: u64,
    pub instructions: Vec<DecodedInsn>,
}

/// Full representation of an analyzed binary.
#[derive(Debug)]
pub struct BinaryProgram {
    pub file_path: String,
    pub format: BinaryFormat,
    pub entry_point: u64,
    pub image_base: u64,
    pub functions: Vec<Function>,
    /// Map from address to function index.
    pub fn_map: HashMap<u64, usize>,
}

/// Binary file format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryFormat {
    Pe,
    Elf,
    MachO,
    Raw,
}

/// Analyze a binary file and produce a program model.
pub fn analyze_binary(file_path: &Path) -> Result<BinaryProgram, String> {
    let data = fs::read(file_path).map_err(|e| format!("failed to read {}: {}", file_path.display(), e))?;
    let file_str = file_path.to_string_lossy().to_string();

    let is_64 = guess_64_bit(&data);

    let mut program = match Object::parse(&data) {
        Ok(Object::PE(pe)) => parse_pe(&file_str, &data, pe, is_64)?,
        Ok(Object::Elf(elf)) => parse_elf(&file_str, &data, elf, is_64)?,
        Ok(Object::Mach(mach)) => parse_macho(&file_str, &data, mach, is_64)?,
        _ => parse_raw(&file_str, &data, is_64)?,
    };

    for i in 0..program.functions.len() {
        let fn_addr = program.functions[i].address;
        let fn_size = program.functions[i].size;
        let (blocks, insns) = disassemble_range(&data, fn_addr, fn_size, is_64);
        program.functions[i].instructions = insns;
        program.functions[i].basic_blocks = blocks;
    }

    Ok(program)
}

fn guess_64_bit(data: &[u8]) -> bool {
    if data.len() > 4 && &data[0..2] == b"MZ" {
        let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;
        if pe_offset + 4 < data.len() {
            let machine = u16::from_le_bytes([data[pe_offset + 4], data[pe_offset + 5]]);
            return machine == 0x8664;
        }
    }
    if data.len() > 5 && &data[0..4] == b"\x7fELF" {
        return data[4] == 2;
    }
    if data.len() > 4 {
        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if magic == 0xFEEDFACF || magic == 0xCFFAEDFE {
            return true;
        }
    }
    true
}

fn parse_pe(file_path: &str, _data: &[u8], pe: PE, _is_64: bool) -> Result<BinaryProgram, String> {
    let entry_point = pe.entry as u64;
    let image_base = pe.image_base as u64;

    let mut functions = Vec::new();

    for export in &pe.exports {
        if let Some(name_str) = &export.name {
            functions.push(Function {
                address: image_base.wrapping_add(export.rva as u64),
                size: export.size,
                name: Some(name_str.to_string()),
                instructions: Vec::new(),
                basic_blocks: Vec::new(),
            });
        }
    }

    if entry_point > 0 {
        functions.push(Function {
            address: entry_point,
            size: 0x200,
            name: Some("entry".into()),
            instructions: Vec::new(),
            basic_blocks: Vec::new(),
        });
    }

    let pe_fn_map: HashMap<u64, usize> = functions.iter().enumerate().map(|(i, f)| (f.address, i)).collect();

    Ok(BinaryProgram {
        file_path: file_path.into(),
        format: BinaryFormat::Pe,
        entry_point,
        image_base,
        functions,
        fn_map: pe_fn_map,
    })
}

fn parse_elf(file_path: &str, _data: &[u8], elf: Elf, _is_64: bool) -> Result<BinaryProgram, String> {
    let entry_point = elf.header.e_entry;
    let image_base: u64 = 0;

    let mut functions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    push_elf_fn(&mut functions, &mut seen, &elf.syms, &elf.strtab, |sym| {
        sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_size > 0
    });
    push_elf_fn(&mut functions, &mut seen, &elf.dynsyms, &elf.dynstrtab, |sym| {
        sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_size > 0
    });

    if entry_point > 0 && !seen.contains(&entry_point) {
        functions.push(new_fn(entry_point, 0x200, Some("_start".into())));
    }

    let elf_fn_map: HashMap<u64, usize> = functions.iter().enumerate().map(|(i, f)| (f.address, i)).collect();

    Ok(BinaryProgram {
        file_path: file_path.into(),
        format: BinaryFormat::Elf,
        entry_point,
        image_base,
        functions,
        fn_map: elf_fn_map,
    })
}

fn push_elf_fn(functions: &mut Vec<Function>, seen: &mut std::collections::HashSet<u64>,
               syms: &goblin::elf::Symtab, strtab: &goblin::strtab::Strtab,
               pred: impl Fn(&goblin::elf::Sym) -> bool) {
    for sym in syms.iter() {
        if !pred(&sym) { continue; }
        if seen.contains(&sym.st_value) { continue; }
        seen.insert(sym.st_value);
        let name = strtab.get_at(sym.st_name).map(|s| s.to_string());
        functions.push(new_fn(sym.st_value, sym.st_size as usize, name));
    }
}

fn new_fn(address: u64, size: usize, name: Option<String>) -> Function {
    Function { address, size, name, instructions: Vec::new(), basic_blocks: Vec::new() }
}

fn parse_macho(file_path: &str, _data: &[u8], _mach: Mach, _is_64: bool) -> Result<BinaryProgram, String> {
    Ok(BinaryProgram {
        file_path: file_path.into(),
        format: BinaryFormat::MachO,
        entry_point: 0,
        image_base: 0,
        functions: Vec::new(),
        fn_map: HashMap::new(),
    })
}

fn parse_raw(file_path: &str, _data: &[u8], _is_64: bool) -> Result<BinaryProgram, String> {
    Ok(BinaryProgram {
        file_path: file_path.into(),
        format: BinaryFormat::Raw,
        entry_point: 0,
        image_base: 0,
        functions: Vec::new(),
        fn_map: HashMap::new(),
    })
}

fn disassemble_range(data: &[u8], address: u64, max_size: usize, is_64: bool) -> (Vec<BasicBlock>, Vec<DecodedInsn>) {
    let bitness = if is_64 { 64 } else { 32 };
    let offset = (address as usize).min(data.len().saturating_sub(1));
    let available = data.len().saturating_sub(offset);
    let size = max_size.min(available);

    if size == 0 {
        return (Vec::new(), Vec::new());
    }

    let mut decoder = Decoder::with_ip(bitness, &data[offset..offset + size], address, DecoderOptions::NONE);
    let mut instructions = Vec::new();
    let mut blocks = Vec::new();
    let mut current_block: Vec<DecodedInsn> = Vec::new();
    let mut block_start = address;

    for _ in 0..2000 {
        let insn = decoder.decode();
        if insn.code() == iced_x86::Code::INVALID {
            break;
        }
        let decoded = decode_single_insn(&insn);
        instructions.push(decoded.clone());
        handle_block_placement(&mut blocks, &mut current_block, &mut block_start, decoded);
    }

    if !current_block.is_empty() {
        flush_block(&mut blocks, block_start, &mut current_block);
    }

    let basic_blocks: Vec<BasicBlock> = blocks.into_iter().map(|b| {
        let end = b.insns.last().map(|i| i.address + i.size as u64).unwrap_or(b.start);
        BasicBlock { start: b.start, end, instructions: b.insns }
    }).collect();

    (basic_blocks, instructions)
}

fn handle_block_placement(blocks: &mut Vec<BuildBlock>, current: &mut Vec<DecodedInsn>,
                          block_start: &mut u64, decoded: DecodedInsn) {
    let is_terminator = decoded.is_call || decoded.is_branch || decoded.is_return;
    let addr = decoded.address;
    let next = addr + decoded.size as u64;
    if !is_terminator {
        current.push(decoded);
    } else if !current.is_empty() {
        current.push(decoded);
        flush_block(blocks, *block_start, current);
    } else {
        blocks.push(BuildBlock { start: addr, insns: vec![decoded] });
    }
    if is_terminator {
        *block_start = next;
    }
}

fn decode_single_insn(insn: &iced_x86::Instruction) -> DecodedInsn {
    let (is_call, is_return, is_branch) = classify_flow(insn);
    let ip = insn.ip();
    let len = insn.len() as usize;
    let text = insn.to_string();
    let (mnemonic, operands) = text.split_once(' ').unwrap_or((&text, ""));
    let call_target = extract_target(is_call, insn);
    let branch_target = extract_target(is_branch, insn);

    DecodedInsn {
        address: ip, size: len,
        mnemonic: mnemonic.to_string(), operands: operands.to_string(),
        is_call, is_branch, is_return,
        call_target, branch_target,
    }
}

fn classify_flow(insn: &iced_x86::Instruction) -> (bool, bool, bool) {
    let flow = insn.flow_control();
    let is_call = flow == FlowControl::Call || flow == FlowControl::IndirectCall;
    let is_return = flow == FlowControl::Return;
    let is_branch = !is_call && !is_return && (flow == FlowControl::UnconditionalBranch
        || flow == FlowControl::IndirectBranch
        || flow == FlowControl::ConditionalBranch);
    (is_call, is_return, is_branch)
}

fn extract_target(enabled: bool, insn: &iced_x86::Instruction) -> Option<u64> {
    if !enabled { return None; }
    let target = insn.near_branch_target();
    if target != 0 { Some(target) } else { None }
}

struct BuildBlock {
    start: u64,
    insns: Vec<DecodedInsn>,
}

fn flush_block(blocks: &mut Vec<BuildBlock>, start: u64, current: &mut Vec<DecodedInsn>) {
    blocks.push(BuildBlock { start, insns: std::mem::take(current) });
}