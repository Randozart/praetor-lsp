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

    for sym in &elf.syms {
        if sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_size > 0 {
            let name = elf.strtab.get_at(sym.st_name).map(|s| s.to_string());
            functions.push(Function {
                address: sym.st_value,
                size: sym.st_size as usize,
                name,
                instructions: Vec::new(),
                basic_blocks: Vec::new(),
            });
        }
    }

    for sym in elf.dynsyms.iter() {
        if sym.st_type() == goblin::elf::sym::STT_FUNC && sym.st_size > 0 {
            if !functions.iter().any(|f| f.address == sym.st_value) {
                let name = elf.dynstrtab.get_at(sym.st_name).map(|s| s.to_string());
                functions.push(Function {
                    address: sym.st_value,
                    size: sym.st_size as usize,
                    name,
                    instructions: Vec::new(),
                    basic_blocks: Vec::new(),
                });
            }
        }
    }

    if entry_point > 0 && !functions.iter().any(|f| f.address == entry_point) {
        functions.push(Function {
            address: entry_point,
            size: 0x200,
            name: Some("_start".into()),
            instructions: Vec::new(),
            basic_blocks: Vec::new(),
        });
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
    let mut block_map: Vec<(u64, Vec<DecodedInsn>)> = Vec::new();
    let mut current_block: Vec<DecodedInsn> = Vec::new();
    let mut block_start = address;

    for _ in 0..2000 {
        let insn = decoder.decode();
        // Stop if the instruction is invalid (end of stream or bad data)
        if insn.code() == iced_x86::Code::INVALID {
            break;
        }

        let flow = insn.flow_control();
        let is_call = flow == FlowControl::Call || flow == FlowControl::IndirectCall;
        let is_return = flow == FlowControl::Return;
        let is_branch = !is_call && !is_return && (flow == FlowControl::UnconditionalBranch
            || flow == FlowControl::IndirectBranch
            || flow == FlowControl::ConditionalBranch);

        let ip = insn.ip();
        let len = insn.len() as usize;
        let text = insn.to_string();

        // Split mnemonic from operands: "call 0x401000" or "mov eax, ebx"
        let (mnemonic, operands) = text.split_once(' ').unwrap_or((&text, ""));

        let call_target = if is_call {
            let target = insn.near_branch_target();
            if target != 0 { Some(target) } else { None }
        } else {
            None
        };
        let branch_target = if is_branch {
            let target = insn.near_branch_target();
            if target != 0 { Some(target) } else { None }
        } else {
            None
        };

        let decoded = DecodedInsn {
            address: ip,
            size: len,
            mnemonic: mnemonic.to_string(),
            operands: operands.to_string(),
            is_call,
            is_branch,
            is_return,
            call_target,
            branch_target,
        };

        if is_branch || is_call || is_return {
            if !current_block.is_empty() {
                current_block.push(decoded.clone());
                block_map.push((block_start, std::mem::take(&mut current_block)));
                block_start = ip + len as u64;
            } else {
                block_map.push((ip, vec![decoded.clone()]));
                block_start = ip + len as u64;
            }
        } else {
            current_block.push(decoded.clone());
        }

        instructions.push(decoded);
    }

    if !current_block.is_empty() {
        block_map.push((block_start, current_block));
    }

    let blocks = block_map.into_iter().map(|(start, insns)| {
        let end = insns.last().map(|i| i.address + i.size as u64).unwrap_or(start);
        BasicBlock { start, end, instructions: insns }
    }).collect();

    (blocks, instructions)
}