use std::sync::LazyLock;

use tree_sitter_language::LanguageFn;

/// Static language configuration: node types for AST traversal.
#[allow(dead_code)]
pub struct LanguageConfig {
    pub name: &'static str,
    pub language_fn: LanguageFn,
    pub function_types: &'static [&'static str],
    pub function_name_path: &'static [&'static str],
    pub loop_types: &'static [&'static str],
    pub call_type: &'static str,
    pub call_target_path: &'static [&'static str],
    pub comment_types: &'static [&'static str],
    pub doc_string_type: Option<&'static str>,
}

// ---------------------------------------------------------------------------
// Language definitions
// ---------------------------------------------------------------------------

macro_rules! lang_cfg {
    ($name:ident, $lang_mod:path, $fn_names:expr, $name_path:expr,
     $loops:expr, $call_type:expr, $call_path:expr, $comments:expr) => {
        pub static $name: LanguageConfig = LanguageConfig {
            name: stringify!($name),
            language_fn: $lang_mod,
            function_types: $fn_names,
            function_name_path: $name_path,
            loop_types: $loops,
            call_type: $call_type,
            call_target_path: $call_path,
            comment_types: $comments,
            doc_string_type: None,
        };
    };
}

lang_cfg!(PYTHON, tree_sitter_python::LANGUAGE,
    &["function_definition", "method_definition"],
    &["identifier"],
    &["for_statement", "while_statement"],
    "call", &["identifier"],
    &["comment"]);

lang_cfg!(JAVASCRIPT, tree_sitter_javascript::LANGUAGE,
    &["function_declaration", "arrow_function", "method_definition"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement"],
    "call_expression", &["identifier", "property_identifier"],
    &["comment"]);

lang_cfg!(TYPESCRIPT, tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
    &["function_declaration", "arrow_function", "method_definition"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement"],
    "call_expression", &["identifier", "property_identifier"],
    &["comment"]);

lang_cfg!(TSX, tree_sitter_typescript::LANGUAGE_TSX,
    &["function_declaration", "arrow_function", "method_definition"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement"],
    "call_expression", &["identifier", "property_identifier"],
    &["comment"]);

lang_cfg!(GO, tree_sitter_go::LANGUAGE,
    &["function_declaration", "method_declaration"],
    &["identifier"],
    &["for_statement", "for_range_clause"],
    "call_expression", &["identifier", "selector_expression"],
    &["comment"]);

lang_cfg!(C_LANG, tree_sitter_c::LANGUAGE,
    &["function_definition"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement"],
    "call_expression", &["identifier"],
    &["comment"]);

lang_cfg!(CPP, tree_sitter_cpp::LANGUAGE,
    &["function_definition"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement", "range_based_for_statement"],
    "call_expression", &["identifier"],
    &["comment"]);

lang_cfg!(RUST, tree_sitter_rust::LANGUAGE,
    &["function_item"],
    &["identifier"],
    &["for_expression", "while_expression", "loop_expression"],
    "call_expression", &["identifier", "scoped_identifier"],
    &["line_comment", "block_comment"]);

lang_cfg!(JAVA, tree_sitter_java::LANGUAGE,
    &["method_declaration"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement", "enhanced_for_statement"],
    "method_invocation", &["identifier"],
    &["line_comment", "block_comment"]);

lang_cfg!(ASM, tree_sitter_asm::LANGUAGE,
    &["label"],
    &["ident"],
    &[],
    "instruction", &["word"],
    &["line_comment", "block_comment"]);
// Note: some labels use "meta_ident" for local symbols (e.g. `.Ltmp0`) — those
// report as (anonymous). Standard labels (`my_label:`) use "ident".

lang_cfg!(SYSTEMVERILOG, tree_sitter_systemverilog::LANGUAGE,
    &["function_declaration"],
    &["function_body_declaration", "simple_identifier"],
    &["loop_statement", "loop_generate_construct"],
    "function_subroutine_call", &["simple_identifier"],
    &["block_comment", "one_line_comment"]);

lang_cfg!(VHDL, tree_sitter_vhdl::LANGUAGE,
    &["subprogram_definition"],
    &["function_specification", "identifier"],
    // Note: Names matching VHDL library functions (e.g. "add") use "library_function"
    // type — those report as (anonymous). Acceptable edge case.
    &["loop_statement"],
    "function_call", &["identifier"],
    &["block_comment", "line_comment"]);

// Ruby: method / singleton_method definitions; calls use "call" node
lang_cfg!(RUBY, tree_sitter_ruby::LANGUAGE,
    &["method", "singleton_method"],
    &["identifier"],
    &["for", "while", "until"],
    "call", &["identifier"],
    &["comment"]);

// Lua: function_declaration; calls use "function_call" node
lang_cfg!(LUA, tree_sitter_lua::LANGUAGE,
    &["function_declaration"],
    &["identifier"],
    &["while_statement", "repeat_statement", "for_statement"],
    "function_call", &["identifier"],
    &["comment"]);

// PHP: function_definition / method_declaration; calls use "function_call_expression"
lang_cfg!(PHP, tree_sitter_php::LANGUAGE_PHP,
    &["function_definition", "method_declaration"],
    &["name", "identifier"],
    &["for_statement", "foreach_statement", "while_statement", "do_statement"],
    "function_call_expression", &["name", "identifier"],
    &["comment"]);

// Kotlin blocked: tree-sitter-kotlin up to v0.3.8 depends on tree-sitter < 0.23,
// which brings in C library symbols that conflict with tree-sitter 0.26.

// Swift: function_declaration
lang_cfg!(SWIFT, tree_sitter_swift::LANGUAGE,
    &["function_declaration"],
    &["identifier"],
    &["for_in_statement", "while_statement", "repeat_while_statement"],
    "call_expression", &["identifier", "member_access_expression"],
    &["comment"]);

// Zig: function_declaration; doc_comment nodes available (future)
lang_cfg!(ZIG, tree_sitter_zig::LANGUAGE,
    &["function_declaration"],
    &["identifier"],
    &["for_expression", "while_expression"],
    "call_expression", &["identifier", "field_access_expression"],
    &["line_comment", "doc_comment"]);

// Dart: function_signature / method_signature for declarations
lang_cfg!(DART, tree_sitter_dart::LANGUAGE,
    &["function_signature", "method_signature", "function_declaration"],
    &["identifier"],
    &["for_statement", "while_statement", "do_statement"],
    "function_expression", &["identifier"],
    &["comment"]);

// Perl: subroutine_declaration_statement
lang_cfg!(PERL, tree_sitter_perl::LANGUAGE,
    &["subroutine_declaration_statement"],
    &["identifier"],
    &["for_statement", "foreach_statement", "while_statement", "until_statement"],
    "function_call_expression", &["identifier"],
    &["comment"]);

// Haskell: function declarations via "function" node, name via "variable"
lang_cfg!(HASKELL, tree_sitter_haskell::LANGUAGE,
    &["function"],
    &["variable"],
    &[],
    "function", &["variable"],
    &["comment"]);

// ---------------------------------------------------------------------------
// Extension → config mapping
// ---------------------------------------------------------------------------

pub fn config_for_extension(ext: &str) -> Option<&'static LanguageConfig> {
    EXT_MAP.get(ext).copied()
}

pub fn all_extensions() -> Vec<&'static str> {
    EXT_MAP.keys().copied().collect()
}

#[allow(dead_code)]
static EXT_MAP: LazyLock<
    std::collections::HashMap<&'static str, &'static LanguageConfig>,
> = LazyLock::new(|| {
    let mut m: std::collections::HashMap<&str, &LanguageConfig> =
        std::collections::HashMap::new();
    m.insert(".py", &PYTHON);
    m.insert(".js", &JAVASCRIPT);
    m.insert(".jsx", &JAVASCRIPT);
    m.insert(".ts", &TYPESCRIPT);
    m.insert(".tsx", &TSX);
    m.insert(".go", &GO);
    m.insert(".c", &C_LANG);
    m.insert(".h", &C_LANG);
    m.insert(".cpp", &CPP);
    m.insert(".cc", &CPP);
    m.insert(".cxx", &CPP);
    m.insert(".hpp", &CPP);
    m.insert(".rs", &RUST);
    m.insert(".java", &JAVA);
    m.insert(".asm", &ASM);
    m.insert(".s", &ASM);
    m.insert(".S", &ASM);
    m.insert(".assembly", &ASM);
    m.insert(".sv", &SYSTEMVERILOG);
    m.insert(".svh", &SYSTEMVERILOG);
    m.insert(".vhd", &VHDL);
    m.insert(".vhdl", &VHDL);
    m.insert(".rb", &RUBY);
    m.insert(".lua", &LUA);
    m.insert(".php", &PHP);
    // m.insert(".kt", &KOTLIN);  // Kotlin blocked — see note above
    // m.insert(".kts", &KOTLIN);
    m.insert(".swift", &SWIFT);
    m.insert(".zig", &ZIG);
    m.insert(".dart", &DART);
    m.insert(".pl", &PERL);
    m.insert(".pm", &PERL);
    m.insert(".hs", &HASKELL);
    m.insert(".lhs", &HASKELL);
    m
});
