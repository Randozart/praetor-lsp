/// Byte-level binary patching engine: NOP sleds, jump redirects, call redirects, shim injection.
use iced_x86::{Decoder, DecoderOptions, FlowControl};

/// Types of patches that can be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PatchKind {
    Nop,
    JumpRedirect,
    CallRedirect,
    ShimInjection,
}

/// A single patch operation on a binary.
#[derive(Debug, Clone)]
pub struct Patch {
    pub address: u64,
    pub kind: PatchKind,
    pub new_bytes: Vec<u8>,
    pub description: String,
}

impl Patch {
    pub fn nop(address: u64, size: usize) -> Self {
        Patch {
            address,
            kind: PatchKind::Nop,
            new_bytes: vec![0x90; size],
            description: format!("NOP sled at {:#x} ({} bytes)", address, size),
        }
    }

    pub fn short_jump(from: u64, to: u64) -> Result<Self, String> {
        let offset = (to as i64).wrapping_sub(from as i64 + 2);
        if offset < -128 || offset > 127 {
            return Err(format!("short jump offset {} out of range", offset));
        }
        let mut bytes = vec![0xEB, offset as u8];
        Patch::finalize_jump_bytes(from, &mut bytes, to)
    }

    pub fn near_jump(from: u64, to: u64, is_64: bool) -> Result<Self, String> {
        if is_64 {
            // `FF 25 00 00 00 00` (jmp [rip+0]) + 8 bytes absolute address
            let mut bytes = vec![0xFF, 0x25, 0x00, 0x00, 0x00, 0x00];
            bytes.extend_from_slice(&(to as u64).to_le_bytes());
            Patch::finalize_jump_bytes(from, &mut bytes, to)
        } else {
            let offset = (to as i64).wrapping_sub(from as i64 + 5);
            let mut bytes = vec![0xE9];
            bytes.extend_from_slice(&(offset as i32).to_le_bytes());
            Patch::finalize_jump_bytes(from, &mut bytes, to)
        }
    }

    pub fn near_call(from: u64, to: u64, is_64: bool) -> Result<Self, String> {
        if is_64 {
            // `FF 15 00 00 00 00` (call [rip+0]) + absolute address
            let mut bytes = vec![0xFF, 0x15, 0x00, 0x00, 0x00, 0x00];
            bytes.extend_from_slice(&(to as u64).to_le_bytes());
            Ok(Patch {
                address: from,
                kind: PatchKind::CallRedirect,
                new_bytes: bytes,
                description: format!("call redirect {:#x} -> {:#x}", from, to),
            })
        } else {
            let offset = (to as i64).wrapping_sub(from as i64 + 5);
            let mut bytes = vec![0xE8];
            bytes.extend_from_slice(&(offset as i32).to_le_bytes());
            Ok(Patch {
                address: from,
                kind: PatchKind::CallRedirect,
                new_bytes: bytes,
                description: format!("call redirect {:#x} -> {:#x}", from, to),
            })
        }
    }

    pub fn shim(address: u64, stub_bytes: Vec<u8>, target_name: &str) -> Self {
        Patch {
            address,
            kind: PatchKind::ShimInjection,
            new_bytes: stub_bytes,
            description: format!("shim injection at {:#x}: {}", address, target_name),
        }
    }

    fn finalize_jump_bytes(from: u64, bytes: &mut Vec<u8>, to: u64) -> Result<Self, String> {
        Ok(Patch {
            address: from,
            kind: PatchKind::JumpRedirect,
            new_bytes: bytes.clone(),
            description: format!("jump redirect {:#x} -> {:#x}", from, to),
        })
    }
}

/// Apply a list of patches to the original binary data.
/// Patches are applied in order; overlapping patches produce an error.
pub fn apply_patches(original: &[u8], patches: &[Patch], image_base: u64) -> Result<Vec<u8>, String> {
    let mut result = original.to_vec();
    let mut sorted = patches.to_vec();
    sorted.sort_by_key(|p| p.address);

    for i in 0..sorted.len() {
        let start = sorted[i].address.wrapping_sub(image_base) as usize;
        let end = start + sorted[i].new_bytes.len();

        if end > result.len() {
            return Err(format!(
                "patch at {:#x} exceeds binary size ({} > {})",
                sorted[i].address, end, result.len()
            ));
        }

        // Check for overlap with previous patch
        if i > 0 {
            let prev_end = sorted[i - 1].address.wrapping_sub(image_base) as usize
                + sorted[i - 1].new_bytes.len();
            if start < prev_end {
                return Err(format!(
                    "overlapping patches at {:#x} and {:#x}",
                    sorted[i - 1].address, sorted[i].address
                ));
            }
        }

        result[start..end].copy_from_slice(&sorted[i].new_bytes);
    }

    Ok(result)
}

/// Generate a trampoline stub for calling an external function.
/// Produces: `jmp [rip+0]; <8 byte absolute addr>` (64-bit) or `jmp [addr]` (32-bit).
pub fn generate_trampoline(target_address: u64, is_64: bool) -> Vec<u8> {
    if is_64 {
        let mut bytes = vec![0xFF, 0x25, 0x00, 0x00, 0x00, 0x00];
        bytes.extend_from_slice(&target_address.to_le_bytes());
        bytes
    } else {
        let mut bytes = vec![0xEA];
        bytes.extend_from_slice(&(target_address as u32).to_le_bytes());
        bytes
    }
}

/// Patch out a call instruction with a NOP sled.
/// Scans backward from address to find the call instruction and measure its size.
pub fn nop_out_call(data: &[u8], call_address: u64, is_64: bool) -> Result<Patch, String> {
    let bitness = if is_64 { 64 } else { 32 };
    let offset = call_address as usize;
    if offset >= data.len() {
        return Err(format!("address {:#x} out of range", call_address));
    }
    let available = data.len() - offset;
    let scan_size = available.min(16);

    let mut decoder = Decoder::with_ip(bitness, &data[offset..offset + scan_size], call_address, DecoderOptions::NONE);
    let insn = decoder.decode();
    if insn.code() == iced_x86::Code::INVALID {
        return Err(format!("invalid instruction at {:#x}", call_address));
    }
    let flow = insn.flow_control();
    if flow != FlowControl::Call && flow != FlowControl::IndirectCall {
        return Err(format!("no call instruction at {:#x} (found {:?})", call_address, flow));
    }
    let size = insn.len() as usize;
    Ok(Patch::nop(call_address, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nop_patch() {
        let patch = Patch::nop(0x401000, 5);
        assert_eq!(patch.new_bytes.len(), 5);
        assert!(patch.new_bytes.iter().all(|&b| b == 0x90));
        assert_eq!(patch.address, 0x401000);
    }

    #[test]
    fn test_short_jump() {
        let patch = Patch::short_jump(0x401000, 0x401020).unwrap();
        assert_eq!(patch.new_bytes[0], 0xEB);
        assert_eq!(patch.new_bytes[1], 0x1E);
    }

    #[test]
    fn test_short_jump_out_of_range() {
        let result = Patch::short_jump(0x401000, 0x500000);
        assert!(result.is_err());
    }

    #[test]
    fn test_near_jump_32bit() {
        let patch = Patch::near_jump(0x401000, 0x500000, false).unwrap();
        assert_eq!(patch.new_bytes[0], 0xE9);
        let offset = i32::from_le_bytes([
            patch.new_bytes[1],
            patch.new_bytes[2],
            patch.new_bytes[3],
            patch.new_bytes[4],
        ]);
        assert_eq!(offset, 0x500000i32 - 0x401000i32 - 5);
    }

    #[test]
    fn test_apply_patches() {
        let original = vec![0x90, 0x90, 0x90, 0x90, 0x90];
        let patches = vec![Patch::nop(0x1000, 3)];
        let result = apply_patches(&original, &patches, 0x1000).unwrap();
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_overlapping_patches_error() {
        let original = vec![0x90; 10];
        let patches = vec![
            Patch::nop(0x1000, 5),
            Patch::nop(0x1003, 3),
        ];
        let result = apply_patches(&original, &patches, 0x1000);
        assert!(result.is_err());
    }
}