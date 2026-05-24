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
    m
});
