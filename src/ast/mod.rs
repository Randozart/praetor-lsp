use std::collections::HashMap;
use std::sync::Mutex;

use tree_sitter::{Node, Parser, Tree};

pub mod languages;

pub use languages::LanguageConfig;

/// Result of parsing a file with tree-sitter.
pub struct ParsedFile<'a> {
    pub tree: Tree,
    pub text: &'a [u8],
    pub config: &'static LanguageConfig,
}

/// Engine that manages tree-sitter parsers for multiple languages.
pub struct AstEngine {
    parsers: Mutex<HashMap<&'static str, Parser>>,
}

impl AstEngine {
    pub fn new() -> Self {
        let mut parsers: HashMap<&str, Parser> = HashMap::new();
        let mut count = 0;

        for ext in languages::all_extensions() {
            if let Some(cfg) = languages::config_for_extension(ext) {
                let mut parser = Parser::new();
                let lang: tree_sitter::Language = cfg.language_fn.into();
                if parser.set_language(&lang).is_ok() {
                    parsers.insert(ext, parser);
                    count += 1;
                }
            }
        }

        tracing::info!("initialized {} language parsers", count);
        Self { parsers: Mutex::new(parsers) }
    }

    /// Number of loaded language parsers.
    pub fn loaded_count(&self) -> usize {
        self.parsers.lock().unwrap().len()
    }

    /// Parse a file's contents. Returns `None` if the extension is not
    /// supported or parsing fails.
    pub fn parse<'a>(&self, ext: &str, text: &'a [u8]) -> Option<ParsedFile<'a>> {
        let mut parsers = self.parsers.lock().ok()?;
        let parser = parsers.get_mut(ext)?;
        let config = languages::config_for_extension(ext)?;
        let tree = parser.parse(text, None)?;
        Some(ParsedFile { tree, text, config })
    }

    /// Checks whether an extension is supported.
    pub fn supports_extension(&self, ext: &str) -> bool {
        self.parsers.lock().unwrap().contains_key(ext)
    }
}

impl Default for AstEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tree-sitter helper utilities
// ---------------------------------------------------------------------------

/// Find the first child matching a path of node types.
pub fn find_child_by_path<'a, 'b>(
    node: Node<'b>,
    path: &[&str],
) -> Option<Node<'b>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == path[0] {
            if path.len() == 1 {
                return Some(child);
            }
            return find_child_by_path(child, &path[1..]);
        }
    }
    None
}

/// Get the text of a node as a string.
pub fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// Compute maximum loop nesting depth within a subtree.
pub fn max_loop_depth(node: Node, loop_types: &[&str], outer: u32) -> u32 {
    let is_loop = loop_types.contains(&node.kind());
    let current = outer + if is_loop { 1 } else { 0 };
    let mut max_d = current;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_depth = max_loop_depth(child, loop_types, current);
        max_d = max_d.max(child_depth);
    }
    max_d
}

/// Check if a function body contains a recursive call to itself.
pub fn has_recursion(
    node: Node,
    fn_name: &str,
    call_type: &str,
    call_target_path: &[&str],
    source: &[u8],
) -> bool {
    if node.kind() == call_type {
        if let Some(target) = find_child_by_path(node, call_target_path) {
            if node_text(target, source) == fn_name {
                return true;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_recursion(child, fn_name, call_type, call_target_path, source) {
            return true;
        }
    }
    false
}
