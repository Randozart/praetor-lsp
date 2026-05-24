use std::collections::HashMap;

use crepe::crepe;
use tree_sitter::Node;

// Reserved IDs for well-known symbols
pub const AUTH_ID: u32 = 0;
pub const PRIV_ID: u32 = 1;
pub const LOG_ID: u32 = 2;
pub const MAIN_ID: u32 = 3;
pub const RUN_ID: u32 = 4;
#[allow(dead_code)]
pub const RESERVED: u32 = 5;

crepe! {
    @input
    struct Call(u32, u32);
    @input
    struct Access(u32, u32);
    @input
    struct Declares(u32, u32);
    @input
    struct Annotated(u32);
    @input
    struct ParamCount(u32, u32);

    @output
    struct Violation(u32, u32, u32);

    // Rule 1: Private data access without prior authenticate or log_access
    Violation(1, f, r) <-
        Access(f, r), (r == PRIV_ID),
        !Call(f, AUTH_ID),
        !Call(f, LOG_ID);

    // Rule 2: Unreachable handler — has docs but no caller
    Violation(2, f, 0) <-
        Annotated(f),
        !Call(_, f),
        (f != MAIN_ID),
        (f != RUN_ID);

    // Rule 3: Declared variable never read
    Violation(3, f, v) <-
        Declares(f, v),
        !Access(f, v);

    // Rule 4: Function with too many parameters (>5)
    Violation(4, f, p) <-
        ParamCount(f, p),
        (p > 5);

    // Rule 5: Function calls a callee that accesses private data without auth
    Violation(5, f, c) <-
        Call(f, c),
        Access(c, PRIV_ID),
        !Call(c, AUTH_ID);
}

pub struct SymbolTable {
    strings: Vec<String>,
    indices: HashMap<String, u32>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut st = Self {
            strings: Vec::new(),
            indices: HashMap::new(),
        };
        st.intern("authenticate");
        st.intern("private_data");
        st.intern("log_access");
        st.intern("main");
        st.intern("run");
        st
    }

    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&idx) = self.indices.get(s) {
            return idx;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.indices.insert(s.to_string(), idx);
        idx
    }

    pub fn resolve(&self, idx: u32) -> &str {
        self.strings.get(idx as usize).map(|s| s.as_str()).unwrap_or("<unknown>")
    }
}

#[derive(Debug, Clone)]
pub struct FactDiagnostic {
    pub rule_id: u32,
    pub function: String,
    #[allow(dead_code)]
    pub detail: String,
    pub message: String,
    /// Line number (0-indexed) for the diagnostic location
    pub line: u32,
    /// Character offset (0-indexed) within the line
    pub character: u32,
}

pub struct FactEngine;

impl FactEngine {
    pub fn analyze(
        parsed: &crate::ast::ParsedFile,
    ) -> Vec<FactDiagnostic> {
        let mut sym = SymbolTable::new();
        let lang = parsed.config;
        let source = parsed.text;
        let root = parsed.tree.root_node();

        let mut calls = Vec::new();
        let mut accesses = Vec::new();
        let mut declares = Vec::new();
        let mut annotated = Vec::new();
        let mut param_counts = Vec::new();
        let mut positions: HashMap<u32, (u32, u32)> = HashMap::new();

        collect_facts(
            root, lang, source, &mut sym,
            &mut calls, &mut accesses, &mut declares,
            &mut annotated, &mut param_counts, &mut positions,
        );

        evaluate_facts(
            &mut sym, &calls, &accesses, &declares,
            &annotated, &param_counts, &positions,
        )
    }
}

/// Run the Crepe Datalog engine with collected facts and return diagnostics.
pub fn evaluate_facts(
    sym: &mut SymbolTable,
    calls: &[(u32, u32)],
    accesses: &[(u32, u32)],
    declares: &[(u32, u32)],
    annotated: &[u32],
    param_counts: &[(u32, u32)],
    positions: &HashMap<u32, (u32, u32)>,
) -> Vec<FactDiagnostic> {
    let mut runtime = Crepe::new();
    runtime.extend(calls.iter().map(|&(a, b)| Call(a, b)));
    runtime.extend(accesses.iter().map(|&(a, b)| Access(a, b)));
    runtime.extend(declares.iter().map(|&(a, b)| Declares(a, b)));
    runtime.extend(annotated.iter().map(|&f| Annotated(f)));
    runtime.extend(param_counts.iter().map(|&(f, c)| ParamCount(f, c)));

    let (violations,) = runtime.run();

    violations
        .into_iter()
        .map(|v| {
            let fn_name = sym.resolve(v.1).to_string();
            let detail = if v.0 == 4 {
                v.2.to_string()
            } else {
                sym.resolve(v.2).to_string()
            };
            let msg = format_message(v.0, &fn_name, &detail);
            let (line, character) = positions.get(&v.1).copied().unwrap_or((0, 0));
            FactDiagnostic {
                rule_id: v.0,
                function: fn_name,
                detail,
                message: msg,
                line,
                character,
            }
        })
        .collect()
}

/// Walk the AST and collect Datalog facts into the provided vectors.
pub fn collect_facts_inner<'a>(
    node: tree_sitter::Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    sym: &mut SymbolTable,
    calls: &mut Vec<(u32, u32)>,
    accesses: &mut Vec<(u32, u32)>,
    declares: &mut Vec<(u32, u32)>,
    annotated: &mut Vec<u32>,
    param_counts: &mut Vec<(u32, u32)>,
    positions: &mut HashMap<u32, (u32, u32)>,
) {
    collect_facts(
        node, lang, source, sym,
        calls, accesses, declares,
        annotated, param_counts, positions,
    );
}

fn format_message(rule_id: u32, fn_name: &str, detail: &str) -> String {
    match rule_id {
        1 => format!(
            "[Datalog Rule 1] `{}` accesses private data without calling \
             authenticate() or logging the access",
            fn_name
        ),
        2 => format!(
            "[Datalog Rule 2] `{}` has documentation but no caller — unreachable handler",
            fn_name
        ),
        3 => format!(
            "[Datalog Rule 3] `{}` declares variable `{}` but never reads it",
            fn_name, detail
        ),
        4 => format!(
            "[Datalog Rule 4] `{}` has {} parameters — consider splitting into \
             smaller functions",
            fn_name, detail
        ),
        5 => format!(
            "[Datalog Rule 5] `{}` calls `{}` which accesses private data without \
             authentication — data leak risk",
            fn_name, detail
        ),
        _ => format!("[Datalog] {} violates rule {}", fn_name, rule_id),
    }
}

fn collect_facts<'a>(
    node: Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    sym: &mut SymbolTable,
    calls: &mut Vec<(u32, u32)>,
    accesses: &mut Vec<(u32, u32)>,
    declares: &mut Vec<(u32, u32)>,
    annotated: &mut Vec<u32>,
    param_counts: &mut Vec<(u32, u32)>,
    positions: &mut HashMap<u32, (u32, u32)>,
) {
    let fn_types = lang.function_types;

    if fn_types.contains(&node.kind()) {
        if let Some(name_node) = crate::ast::find_child_by_path(node, lang.function_name_path) {
            let name = crate::ast::node_text(name_node, source);
            if name.is_empty() {
                return;
            }
            let fn_id = sym.intern(name);

            // Store function position
            let start = name_node.start_position();
            positions.insert(fn_id, (start.row as u32, start.column as u32));

            // Check for preceding comment
            if let Some(prev) = previous_sibling(node) {
                if lang.comment_types.contains(&prev.kind()) {
                    annotated.push(fn_id);
                }
            }

            // Count parameters
            let mut p_cursor = node.walk();
            for child in node.children(&mut p_cursor) {
                if child.kind() == "parameters" {
                    let param_count = count_logical_params(child);
                    param_counts.push((fn_id, param_count));
                }
            }

            // Detect calls, accesses, and declarations within function body
            collect_calls_and_accesses(
                node, fn_id, lang, source, sym, calls, accesses, declares,
            );
        }
    }

    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_facts(
                child, lang, source, sym,
                calls, accesses, declares, annotated, param_counts, positions,
            );
        }
    }
}

fn collect_calls_and_accesses<'a>(
    node: tree_sitter::Node<'a>,
    fn_id: u32,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    sym: &mut SymbolTable,
    calls: &mut Vec<(u32, u32)>,
    accesses: &mut Vec<(u32, u32)>,
    declares: &mut Vec<(u32, u32)>,
) {
    if node.kind() == lang.call_type {
        if let Some(target) = crate::ast::find_child_by_path(node, lang.call_target_path) {
            let callee = crate::ast::node_text(target, source);
            let callee_id = sym.intern(callee);
            calls.push((fn_id, callee_id));
        }
    }

    let kind = node.kind();
    if kind == "assignment" || kind == "variable_declaration"
        || kind == "let_declaration" || kind == "lexical_declaration"
    {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "identifier" {
                let var_name = crate::ast::node_text(child, source);
                let var_id = sym.intern(var_name);
                declares.push((fn_id, var_id));
            }
        }
    }

    if kind == "identifier" {
        let name = crate::ast::node_text(node, source);
        let res_id = sym.intern(name);
        accesses.push((fn_id, res_id));
    }

    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_calls_and_accesses(
                child, fn_id, lang, source, sym, calls, accesses, declares,
            );
        }
    }
}

fn previous_sibling(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = node.walk();
    if !cursor.goto_parent() {
        return None;
    }
    let parent = cursor.node();
    let mut prev: Option<tree_sitter::Node> = None;
    let mut c = parent.walk();
    for child in parent.children(&mut c) {
        if child == node {
            return prev;
        }
        prev = Some(child);
    }
    None
}

/// Count logical parameters by counting child nodes that look like parameters
/// (not commas, colons, type annotations, or parentheses).
fn count_logical_params(node: tree_sitter::Node) -> u32 {
    let mut c = node.walk();
    let mut count = 0;
    for child in node.children(&mut c) {
        let kind = child.kind();
        match kind {
            "," | ":" | "(" | ")" | "->" => {}
            _ if kind.ends_with("type") || kind.ends_with("annotation") => {}
            _ => {
                count += 1;
            }
        }
    }
    count
}
