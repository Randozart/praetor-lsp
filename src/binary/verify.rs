/// CFG topology equivalence verifier: compares original and patched binary CFGs.
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::binary::lift::{self, BinaryProgram, Function};
use crate::binary::patch::Patch;

/// Result of comparing two binary CFGs.
#[derive(Debug)]
pub struct TopologyReport {
    pub original: BinaryProgram,
    pub patched: BinaryProgram,
    pub total_original_fns: usize,
    pub total_patched_fns: usize,
    pub matched_functions: usize,
    pub new_functions: usize,
    pub removed_functions: usize,
    pub changed_functions: Vec<FnDiff>,
    pub preserved_edges: usize,
    pub new_edges: usize,
    pub removed_edges: usize,
    pub patch_impact: Vec<PatchImpact>,
}

/// Difference for a single function between original and patched.
#[derive(Debug, Clone)]
pub struct FnDiff {
    pub name: String,
    pub original_address: u64,
    pub patched_address: Option<u64>,
    pub original_blocks: usize,
    pub patched_blocks: usize,
    pub original_insns: usize,
    pub patched_insns: usize,
    pub status: FnStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FnStatus {
    Unchanged,
    Modified,
    Added,
    Removed,
}

/// How a patch affects the topology.
#[derive(Debug, Clone)]
pub struct PatchImpact {
    pub address: u64,
    pub description: String,
    pub original_flow: String,
    pub patched_flow: String,
    pub topology_preserved: bool,
}

/// Compare two binaries: original and patched.
/// Returns a topology report showing what changed.
pub fn compare_binaries(original_path: &Path, patched_path: &Path) -> Result<TopologyReport, String> {
    let original = lift::analyze_binary(original_path)?;
    let patched = lift::analyze_binary(patched_path)?;

    let mut report = TopologyReport {
        total_original_fns: original.functions.len(),
        total_patched_fns: patched.functions.len(),
        matched_functions: 0,
        new_functions: 0,
        removed_functions: 0,
        changed_functions: Vec::new(),
        preserved_edges: 0,
        new_edges: 0,
        removed_edges: 0,
        patch_impact: Vec::new(),
        original,
        patched,
    };

    match_functions(&mut report);
    compare_edges(&mut report);

    Ok(report)
}

fn match_functions(report: &mut TopologyReport) {
    let orig_fn_map: HashMap<u64, &Function> = report.original.fn_map.iter()
        .map(|(&addr, &idx)| (addr, &report.original.functions[idx])).collect();
    let patched_fn_map: HashMap<u64, &Function> = report.patched.fn_map.iter()
        .map(|(&addr, &idx)| (addr, &report.patched.functions[idx])).collect();

    let mut matched_patch = HashSet::new();

    for (&addr, orig_fn) in &orig_fn_map {
        if let Some(patch_fn) = patched_fn_map.get(&addr) {
            matched_patch.insert(addr);
            let diff = compare_functions(orig_fn, Some(patch_fn));
            report.matched_functions += 1;
            if diff.status == FnStatus::Modified {
                report.changed_functions.push(diff);
            }
        } else {
            report.removed_functions += 1;
            report.changed_functions.push(compare_functions(orig_fn, None));
        }
    }

    for (&addr, patch_fn) in &patched_fn_map {
        if !matched_patch.contains(&addr) {
            report.new_functions += 1;
            report.changed_functions.push(compare_functions(patch_fn, None));
        }
    }
}

fn compare_edges(report: &mut TopologyReport) {
    let orig_edges = extract_call_edges(&report.original);
    let patched_edges = extract_call_edges(&report.patched);

    for edge in &orig_edges {
        if patched_edges.contains(edge) {
            report.preserved_edges += 1;
        } else {
            report.removed_edges += 1;
        }
    }
    for edge in &patched_edges {
        if !orig_edges.contains(edge) {
            report.new_edges += 1;
        }
    }
}

/// Compare a function in original vs patched.
fn compare_functions(orig: &Function, patched: Option<&Function>) -> FnDiff {
    let name = orig.name.clone().unwrap_or_else(|| format!("func_{:#x}", orig.address));

    match patched {
        Some(p) => {
            let blocks_match = orig.basic_blocks.len() == p.basic_blocks.len();
            let insns_match = orig.instructions.len() == p.instructions.len();

            if blocks_match && insns_match {
                // Deep compare: check each block's start/end/instructions
                let deep_match = orig.basic_blocks.iter().zip(p.basic_blocks.iter()).all(|(a, b)| {
                    a.start == b.start
                        && a.end == b.end
                        && a.instructions.len() == b.instructions.len()
                });
                if deep_match {
                    FnDiff {
                        name,
                        original_address: orig.address,
                        patched_address: Some(p.address),
                        original_blocks: orig.basic_blocks.len(),
                        patched_blocks: p.basic_blocks.len(),
                        original_insns: orig.instructions.len(),
                        patched_insns: p.instructions.len(),
                        status: FnStatus::Unchanged,
                    }
                } else {
                    FnDiff {
                        name,
                        original_address: orig.address,
                        patched_address: Some(p.address),
                        original_blocks: orig.basic_blocks.len(),
                        patched_blocks: p.basic_blocks.len(),
                        original_insns: orig.instructions.len(),
                        patched_insns: p.instructions.len(),
                        status: FnStatus::Modified,
                    }
                }
            } else {
                FnDiff {
                    name,
                    original_address: orig.address,
                    patched_address: Some(p.address),
                    original_blocks: orig.basic_blocks.len(),
                    patched_blocks: p.basic_blocks.len(),
                    original_insns: orig.instructions.len(),
                    patched_insns: p.instructions.len(),
                    status: FnStatus::Modified,
                }
            }
        }
        None => FnDiff {
            name,
            original_address: orig.address,
            patched_address: None,
            original_blocks: orig.basic_blocks.len(),
            patched_blocks: 0,
            original_insns: orig.instructions.len(),
            patched_insns: 0,
            status: FnStatus::Removed,
        },
    }
}

/// A call edge (from, to) pair.
type CallEdge = (u64, u64);

/// Extract all call edges from a binary program.
fn extract_call_edges(program: &BinaryProgram) -> HashSet<CallEdge> {
    program.functions.iter()
        .flat_map(|f| f.instructions.iter())
        .filter(|i| i.is_call)
        .filter_map(|i| i.call_target.map(|t| (i.address, t)))
        .collect()
}

/// Verify that applying a set of patches preserves the overall CFG topology.
pub fn verify_patches(original_path: &Path, _patches: &[Patch], patched_path: &Path) -> Result<TopologyReport, String> {
    compare_binaries(original_path, patched_path)
}

/// Generate a human-readable report from a topology comparison.
pub fn format_topology_report(report: &TopologyReport) -> String {
    let mut out = String::new();
    let orig_count: usize = report.original.functions.iter()
        .flat_map(|f| f.instructions.iter())
        .filter(|i| i.is_call && i.call_target.is_some())
        .count();
    let patched_count: usize = report.patched.functions.iter()
        .flat_map(|f| f.instructions.iter())
        .filter(|i| i.is_call && i.call_target.is_some())
        .count();

    out.push_str("# CFG Topology Verification\n\n");
    out.push_str(&format!("Original: {} functions, {} call edges\n",
        report.total_original_fns, orig_count));
    out.push_str(&format!("Patched:  {} functions, {} call edges\n",
        report.total_patched_fns, patched_count));
    out.push_str("\n## Summary\n\n");
    out.push_str(&format!("- Matched (unchanged): {}\n", report.matched_functions));
    out.push_str(&format!("- Modified: {}\n", report.changed_functions.iter().filter(|d| d.status == FnStatus::Modified).count()));
    out.push_str(&format!("- Added: {}\n", report.new_functions));
    out.push_str(&format!("- Removed: {}\n", report.removed_functions));
    out.push_str(&format!("- Preserved edges: {}\n", report.preserved_edges));
    out.push_str(&format!("- New edges: {}\n", report.new_edges));
    out.push_str(&format!("- Removed edges: {}\n", report.removed_edges));

    // Show changes
    let changes: Vec<_> = report.changed_functions.iter().filter(|d| d.status != FnStatus::Unchanged).collect();
    if !changes.is_empty() {
        out.push_str("\n## Changes\n\n");
        out.push_str("| Status | Name | Address | Orig Blocks | Patch Blocks | Orig Insns | Patch Insns |\n");
        out.push_str("|--------|------|---------|-------------|--------------|------------|-------------|\n");
        for d in &changes {
            let addr = d.original_address;
            let paddr = d.patched_address.map(|a| format!("{:#x}", a)).unwrap_or_else(|| "-".into());
            out.push_str(&format!(
                "| {:?} | {} | {:#x}->{} | {} | {} | {} | {} |\n",
                d.status, d.name, addr, paddr,
                d.original_blocks, d.patched_blocks,
                d.original_insns, d.patched_insns,
            ));
        }
    }

    let total = extract_call_edges(&report.original).len() as f64;
    let preserved_ratio = if total > 0.0 {
        report.preserved_edges as f64 / total
    } else {
        1.0
    };
    out.push_str(&format!("\n## Verdict\n\n"));
    if preserved_ratio >= 0.95 && report.new_edges <= 10 {
        out.push_str(&format!("[PASS] Topology preserved ({:.1}% edges unchanged)\n", preserved_ratio * 100.0));
    } else {
        out.push_str(&format!("[WARN] Topology changed significantly ({:.1}% edges preserved, {} new edges)\n",
            preserved_ratio * 100.0, report.new_edges));
    }

    out
}