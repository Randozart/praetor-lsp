#!/usr/bin/env python3
"""Tree-sitter based Big-O complexity analyzer LSP.

Deterministically estimates time complexity from AST structure:
- Loop nesting depth → O(n^k)
- Recursion without memoization → O(2^n)
- Linear operations inside loops → O(n*m)
- Known algorithm anti-patterns

Reports inlay hints at function signatures for ALL functions.
"""

import logging
import os
from pathlib import Path

from pygls.lsp.server import LanguageServer
from lsprotocol.types import (
    InlayHint,
    InlayHintKind,
    InlayHintLabelPart,
    InlayHintParams,
    Position,
    Range,
)
from tree_sitter import Language, Node, Parser, Tree

SERVER = LanguageServer("complexity-lsp", "v0.1")

# ---------------------------------------------------------------------------
# Language configuration: maps extension → tree-sitter language details
# ---------------------------------------------------------------------------

LANG_CONFIG = {
    ".py": {
        "module": "tree_sitter_python",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement"},
        "call_type": "call",
        "call_target_path": ["identifier"],
    },
    ".js": {
        "module": "tree_sitter_javascript",
        "function_types": {"function_declaration", "arrow_function", "method_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".ts": {
        "module": "tree_sitter_typescript",
        "function_types": {"function_declaration", "arrow_function", "method_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".tsx": {
        "module": "tree_sitter_typescript",
        "function_types": {"function_declaration", "arrow_function", "method_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".jsx": {
        "module": "tree_sitter_javascript",
        "function_types": {"function_declaration", "arrow_function", "method_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".go": {
        "module": "tree_sitter_go",
        "function_types": {"function_declaration"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "for_range_clause"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".c": {
        "module": "tree_sitter_c",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".cpp": {
        "module": "tree_sitter_cpp",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "range_based_for_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".cc": {
        "module": "tree_sitter_cpp",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "range_based_for_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".cxx": {
        "module": "tree_sitter_cpp",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "range_based_for_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".h": {
        "module": "tree_sitter_c",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".hpp": {
        "module": "tree_sitter_cpp",
        "function_types": {"function_definition"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "range_based_for_statement"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".rs": {
        "module": "tree_sitter_rust",
        "function_types": {"function_item"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_expression", "while_expression", "loop_expression"},
        "call_type": "call_expression",
        "call_target_path": ["identifier"],
    },
    ".java": {
        "module": "tree_sitter_java",
        "function_types": {"method_declaration"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "enhanced_for_statement"},
        "call_type": "method_invocation",
        "call_target_path": ["identifier"],
    },
    ".rb": {
        "module": "tree_sitter_ruby",
        "function_types": {"method", "singleton_method"},
        "function_name_path": ["identifier"],
        "loop_types": {"for", "while", "until"},
        "call_type": "call",
        "call_target_path": ["identifier"],
    },
    ".cs": {
        "module": "tree_sitter_c_sharp",
        "function_types": {"method_declaration"},
        "function_name_path": ["identifier"],
        "loop_types": {"for_statement", "while_statement", "do_statement", "foreach_statement"},
        "call_type": "invocation_expression",
        "call_target_path": ["identifier"],
    },
}

# ---------------------------------------------------------------------------
# Cached parsers per extension
# ---------------------------------------------------------------------------

_parsers: dict[str, Parser] = {}

LINEAR_OPS = {"indexOf", "find", "contains", "includes", "search", "index", "count"}


def _get_parser(ext: str) -> Parser | None:
    if ext in _parsers:
        return _parsers[ext]
    cfg = LANG_CONFIG.get(ext)
    if cfg is None:
        return None
    try:
        mod = __import__(cfg["module"], fromlist=["language"])
        lang = Language(mod.language())
        parser = Parser(lang)
        _parsers[ext] = parser
        return parser
    except Exception as exc:
        logging.warning("Failed to load %s: %s", cfg["module"], exc)
        return None


# ---------------------------------------------------------------------------
# Complexity analysis
# ---------------------------------------------------------------------------

class ComplexityResult:
    def __init__(self, label: str, detail: str = ""):
        self.label = label
        self.detail = detail


def _get_node_text(node: Node) -> str:
    raw = node.text
    if raw is None:
        return ""
    try:
        return raw.decode("utf-8", errors="replace")
    except Exception:
        return ""


def _find_child_by_path(node: Node, path: list[str]) -> Node | None:
    for child in node.children:
        if child.type == path[0]:
            if len(path) == 1:
                return child
            return _find_child_by_path(child, path[1:])
    return None


def _max_loop_depth(node: Node, loop_types: set[str], outer: int = 0) -> int:
    if node.type in loop_types:
        outer += 1
    max_d = outer
    for child in node.children:
        child_depth = _max_loop_depth(child, loop_types, outer)
        max_d = max(max_d, child_depth)
    return max_d


def _has_recursion(node: Node, fn_name: str | None, cfg: dict) -> bool:
    if fn_name is None:
        return False
    call_target = cfg.get("call_target_path", ["identifier"])
    if node.type == cfg["call_type"]:
        target = _find_child_by_path(node, call_target)
        if target and _get_node_text(target) == fn_name:
            return True
    for child in node.children:
        if _has_recursion(child, fn_name, cfg):
            return True
    return False


def _linear_ops_in_loops(node: Node, loop_types: set[str], cfg: dict) -> int:
    count = 0
    if node.type in loop_types:
        count += _count_linear_ops_in_body(node, cfg)
    for child in node.children:
        count += _linear_ops_in_loops(child, loop_types, cfg)
    return count


def _count_linear_ops_in_body(node: Node, cfg: dict) -> int:
    count = 0
    if node.type == cfg["call_type"]:
        target = _find_child_by_path(node, cfg.get("call_target_path", ["identifier"]))
        if target and _get_node_text(target) in LINEAR_OPS:
            count += 1
    for child in node.children:
        count += _count_linear_ops_in_body(child, cfg)
    return count


def _analyze_function(fn_node: Node, ext: str) -> ComplexityResult | None:
    cfg = LANG_CONFIG.get(ext)
    if cfg is None:
        return None

    fn_name = None
    name_node = _find_child_by_path(fn_node, cfg["function_name_path"])
    if name_node:
        fn_name = _get_node_text(name_node)

    loop_depth = _max_loop_depth(fn_node, cfg["loop_types"])
    has_rec = _has_recursion(fn_node, fn_name, cfg)
    linear_ops = _linear_ops_in_loops(fn_node, cfg["loop_types"], cfg)

    detail_parts: list[str] = []
    label = "O(1)"

    if has_rec:
        label = "O(2\u207f)"
        detail_parts.append("recursive")
    elif loop_depth >= 3:
        label = f"O(n\u00b3)"
        detail_parts.append(f"loop depth {loop_depth}")
    elif loop_depth == 2:
        if linear_ops > 0:
            label = "O(n\u00b2\u00b7m)"
            detail_parts.append(f"nested loops + linear ops ({linear_ops})")
        else:
            label = "O(n\u00b2)"
            detail_parts.append(f"loop depth {loop_depth}")
    elif loop_depth == 1:
        if linear_ops > 0:
            label = "O(n\u00b7m)"
            detail_parts.append(f"linear ops in loop ({linear_ops})")
        else:
            label = "O(n)"
            detail_parts.append("single loop")
    elif linear_ops > 0:
        label = "O(n)"
        detail_parts.append(f"linear operation ({linear_ops})")
    else:
        label = "O(1)"
        detail_parts.append("constant")

    detail = "; ".join(detail_parts)
    return ComplexityResult(label=label, detail=detail)


# ---------------------------------------------------------------------------
# LSP handlers
# ---------------------------------------------------------------------------

def _get_inlay_hints(uri: str) -> list[InlayHint]:
    filepath = uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return []

    ext = Path(filepath).suffix.lower()
    parser = _get_parser(ext)
    if parser is None:
        return []

    try:
        with open(filepath, "rb") as f:
            code = f.read()
    except OSError:
        return []

    tree: Tree = parser.parse(code)
    root = tree.root_node

    cfg = LANG_CONFIG.get(ext)
    if cfg is None:
        return []

    hints: list[InlayHint] = []

    def walk(node: Node):
        if node.type in cfg["function_types"]:
            result = _analyze_function(node, ext)
            if result:
                name_node = _find_child_by_path(node, cfg["function_name_path"])
                if name_node:
                    end_pos = name_node.end_point
                    pos = Position(line=end_pos[0], character=end_pos[1] + 1)
                    hints.append(InlayHint(
                        position=pos,
                        label=[InlayHintLabelPart(
                            value=f" \u26a1 {result.label}",
                            tooltip=f"Estimated complexity: {result.label} ({result.detail})",
                        )],
                        kind=InlayHintKind.Type,
                        padding_right=True,
                    ))
        for child in node.children:
            walk(child)

    walk(root)
    return hints


@SERVER.feature("textDocument/inlayHint")
def on_inlay_hint(ls, params: InlayHintParams):
    uri = params.text_document.uri
    return _get_inlay_hints(uri)


if __name__ == "__main__":
    logging.basicConfig(level=logging.WARNING)
    SERVER.start_io()
