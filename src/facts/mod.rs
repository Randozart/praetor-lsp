use std::collections::HashMap;

use crepe::crepe;

// Reserved IDs for well-known symbols (fallback when no config provided)
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
    @input
    struct AuthFn(u32);
    @input
    struct PrivateLabel(u32);
    @input
    struct LogFn(u32);
    @input
    struct EntryPt(u32);

    @output
    struct Violation(u32, u32, u32);

    // Rule 1: Private data access without prior authenticate or log_access
    Violation(1, f, r) <-
        Access(f, r), PrivateLabel(r),
        AuthFn(x), !Call(f, x),
        LogFn(y), !Call(f, y);

    // Rule 2: Unreachable handler — has docs but no caller
    Violation(2, f, 0) <-
        Annotated(f),
        !Call(_, f),
        !EntryPt(f);

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

#[derive(Default)]
pub struct SymbolTable {
    strings: Vec<String>,
    indices: HashMap<String, u32>,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut st = Self::default();
        st.intern("authenticate");
        st.intern("private_data");
        st.intern("log_access");
        st.intern("main");
        st.intern("run");
        st
    }

    /// Build a SymbolTable from DatalogConfig, interning configured names.
    pub fn with_config(cfg: &crate::config::DatalogConfig) -> Self {
        let mut st = Self::default();
        for name in &cfg.auth_functions { st.intern(name); }
        for name in &cfg.private_data_labels { st.intern(name); }
        for name in &cfg.log_functions { st.intern(name); }
        for name in &cfg.entry_points { st.intern(name); }
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

/// Context holding all Datalog fact-collection state.
/// Replaces 7+ separate vector parameters with a single struct.
#[derive(Default)]
pub struct FactContext {
    pub sym: SymbolTable,
    pub calls: Vec<(u32, u32)>,
    pub accesses: Vec<(u32, u32)>,
    pub declares: Vec<(u32, u32)>,
    pub annotated: Vec<u32>,
    pub param_counts: Vec<(u32, u32)>,
    pub positions: HashMap<u32, (u32, u32)>,
}

pub struct FactEngine;

impl FactEngine {
    pub fn analyze(
        parsed: &crate::ast::ParsedFile,
        datalog_config: Option<&crate::config::DatalogConfig>,
    ) -> Vec<FactDiagnostic> {
        let lang = parsed.config;
        let source = parsed.text;
        let root = parsed.tree.root_node();
        let mut ctx = FactContext::default();
        ctx.sym = match datalog_config {
            Some(cfg) => SymbolTable::with_config(cfg),
            None => SymbolTable::new(),
        };

        collect_facts(root, lang, source, &mut ctx);

        evaluate_facts(&mut ctx, datalog_config)
    }
}

/// Run the Crepe Datalog engine with collected facts and return diagnostics.
pub fn evaluate_facts(
    ctx: &mut FactContext,
    datalog_config: Option<&crate::config::DatalogConfig>,
) -> Vec<FactDiagnostic> {
    let sym = &mut ctx.sym;
    let mut runtime = Crepe::new();
    runtime.extend(ctx.calls.iter().map(|&(a, b)| Call(a, b)));
    runtime.extend(ctx.accesses.iter().map(|&(a, b)| Access(a, b)));
    runtime.extend(ctx.declares.iter().map(|&(a, b)| Declares(a, b)));
    runtime.extend(ctx.annotated.iter().map(|&f| Annotated(f)));
    runtime.extend(ctx.param_counts.iter().map(|&(f, c)| ParamCount(f, c)));

    // Populate configurable Datalog relations
    let cfg = datalog_config.cloned().unwrap_or_default();
    for name in &cfg.auth_functions {
        let id = sym.intern(name);
        runtime.extend(std::iter::once(AuthFn(id)));
    }
    for name in &cfg.private_data_labels {
        let id = sym.intern(name);
        runtime.extend(std::iter::once(PrivateLabel(id)));
    }
    for name in &cfg.log_functions {
        let id = sym.intern(name);
        runtime.extend(std::iter::once(LogFn(id)));
    }
    for name in &cfg.entry_points {
        let id = sym.intern(name);
        runtime.extend(std::iter::once(EntryPt(id)));
    }
    // Fallback: if no config, use hardcoded reserved IDs
    if cfg.auth_functions.is_empty() {
        runtime.extend(std::iter::once(AuthFn(AUTH_ID)));
    }
    if cfg.private_data_labels.is_empty() {
        runtime.extend(std::iter::once(PrivateLabel(PRIV_ID)));
    }
    if cfg.log_functions.is_empty() {
        runtime.extend(std::iter::once(LogFn(LOG_ID)));
    }
    if cfg.entry_points.is_empty() {
        runtime.extend(std::iter::once(EntryPt(MAIN_ID)));
        runtime.extend(std::iter::once(EntryPt(RUN_ID)));
    }

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
            let (line, character) = ctx.positions.get(&v.1).copied().unwrap_or((0, 0));
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

/// Walk the AST and collect Datalog facts into the provided context.
pub fn collect_facts_inner<'a>(
    node: tree_sitter::Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    ctx: &mut FactContext,
) {
    collect_facts(node, lang, source, ctx);
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
    node: tree_sitter::Node<'a>,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    ctx: &mut FactContext,
) {
    let fn_types = lang.function_types;

    if fn_types.contains(&node.kind()) {
        if let Some(name_node) = crate::ast::find_child_by_path(node, lang.function_name_path) {
            let name = crate::ast::node_text(name_node, source);
            if name.is_empty() {
                return;
            }
            let fn_id = ctx.sym.intern(name);

            let start = name_node.start_position();
            ctx.positions.insert(fn_id, (start.row as u32, start.column as u32));

            if let Some(prev) = crate::ast::previous_sibling(node) {
                if lang.comment_types.contains(&prev.kind()) {
                    ctx.annotated.push(fn_id);
                }
            }

            let mut p_cursor = node.walk();
            for child in node.children(&mut p_cursor) {
                if child.kind() == "parameters" {
                    let param_count = count_logical_params(child);
                    ctx.param_counts.push((fn_id, param_count));
                }
            }

            collect_calls_and_accesses(node, fn_id, lang, source, ctx);
        }
    }

    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_facts(child, lang, source, ctx);
        }
    }
}

fn collect_calls_and_accesses<'a>(
    node: tree_sitter::Node<'a>,
    fn_id: u32,
    lang: &crate::ast::LanguageConfig,
    source: &'a [u8],
    ctx: &mut FactContext,
) {
    if node.kind() == lang.call_type {
        if let Some(target) = crate::ast::find_child_by_path(node, lang.call_target_path) {
            let callee = crate::ast::node_text(target, source);
            let callee_id = ctx.sym.intern(callee);
            ctx.calls.push((fn_id, callee_id));
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
                let var_id = ctx.sym.intern(var_name);
                ctx.declares.push((fn_id, var_id));
            }
        }
    }

    if kind == "identifier" {
        let name = crate::ast::node_text(node, source);
        let res_id = ctx.sym.intern(name);
        ctx.accesses.push((fn_id, res_id));
    }

    if node.child_count() > 0 {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            collect_calls_and_accesses(child, fn_id, lang, source, ctx);
        }
    }
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
