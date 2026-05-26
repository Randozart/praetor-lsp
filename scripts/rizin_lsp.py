#!/usr/bin/env python3
"""Rizin-based binary analysis LSP bridge.

Provides disassembly, symbol navigation, and cross-references
for binary files (.dll, .exe, .so, .o, .bin, .elf) via rizin/r2pipe.

Requires:
  - rizin binary (installed via `praetor setup` or manually)
  - r2pipe Python package (pip3 install --user r2pipe)
"""

import json
import logging
import os
import re
import subprocess
import tempfile
from pathlib import Path

from pygls.lsp.server import LanguageServer
from lsprotocol.types import (
    Diagnostic,
    DiagnosticSeverity,
    Hover,
    HoverContents,
    Location,
    MarkupContent,
    MarkupKind,
    Position,
    Range,
    SymbolInformation,
    SymbolKind,
    TextDocumentPositionParams,
    WorkspaceSymbolParams,
)

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("rizin-lsp")

SERVER = LanguageServer("rizin-lsp", "v0.1")

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _find_rizin() -> str | None:
    """Locate the rizin binary — check PATH then praetor cache."""
    for name in ("rizin", "r2"):
        try:
            result = subprocess.run(
                [name, "-v"], capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                return name
        except (FileNotFoundError, subprocess.TimeoutExpired):
            continue
    # Check praetor cache
    cache = Path.home() / ".praetor-lsp" / "bin" / "rizin"
    if cache.exists():
        return str(cache)
    return None


def _run_rizin_cmd(filepath: str, cmd: str, rizin_bin: str) -> str:
    """Run a rizin command on the given binary and return output."""
    try:
        result = subprocess.run(
            [rizin_bin, "-q", "-c", cmd, filepath],
            capture_output=True, text=True, timeout=30,
        )
        return result.stdout
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        logger.warning("rizin command failed: %s", e)
        return ""


# ---------------------------------------------------------------------------
# Capabilities
# ---------------------------------------------------------------------------

@SERVER.feature("textDocument/documentSymbol")
def document_symbols(ls, params):
    """Return all functions as document symbols."""
    filepath = params.text_document.uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return None

    rizin_bin = _find_rizin()
    if rizin_bin is None:
        return None

    output = _run_rizin_cmd(filepath, "afl", rizin_bin)
    symbols: list[SymbolInformation] = []
    for line in output.strip().split("\n"):
        if not line.strip():
            continue
        # Parse typical afl output: 0x401000 64 entry0
        parts = line.strip().split()
        if len(parts) >= 2:
            addr_str = parts[0]
            try:
                addr = int(addr_str, 16)
            except ValueError:
                continue
            name = parts[-1]
            symbols.append(SymbolInformation(
                name=name,
                kind=SymbolKind.Function,
                location=Location(
                    uri=params.text_document.uri,
                    range=Range(
                        start=Position(line=0, character=0),
                        end=Position(line=0, character=1),
                    ),
                ),
            ))
    return symbols


@SERVER.feature("textDocument/hover")
def hover(ls, params: TextDocumentPositionParams):
    """Show disassembly at hovered symbol or address."""
    filepath = params.text_document.uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return None

    rizin_bin = _find_rizin()
    if rizin_bin is None:
        return None

    # Get word at cursor from the document text
    doc = ls.workspace.get_text_document(params.text_document.uri)
    if doc is None:
        return None
    lines = doc.source.split("\n")
    line_idx = params.position.line
    if line_idx >= len(lines):
        return None
    line = lines[line_idx]
    col = params.position.character

    # Extract the word at cursor
    word_match = re.search(r'([a-zA-Z_][a-zA-Z0-9_]*)', line[col:])
    addr_match = re.search(r'(0x[0-9a-fA-F]+)', line[max(0, col - 10):col + 10])

    word = word_match.group(1) if word_match else None
    addr_str = addr_match.group(1) if addr_match else None

    hover_text = ""
    if word:
        # Try as a symbol name
        output = _run_rizin_cmd(filepath, f"pdf @ {word} ~10", rizin_bin)
        if output.strip():
            hover_text += f"### `{word}`\n```\n{output.strip()[:2000]}\n```\n"
    if addr_str and not hover_text:
        # Try as an address
        output = _run_rizin_cmd(filepath, f"pdi 10 @ {addr_str}", rizin_bin)
        if output.strip():
            hover_text += f"### `{addr_str}`\n```\n{output.strip()[:2000]}\n```\n"
    if not hover_text:
        return None

    return Hover(
        contents=HoverContents(
            kind=MarkupKind.Markdown,
            value=hover_text.strip(),
        ),
    )


@SERVER.feature("textDocument/definition")
def goto_definition(ls, params: TextDocumentPositionParams):
    """Jump to function definition at the symbol under cursor."""
    filepath = params.text_document.uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return None

    rizin_bin = _find_rizin()
    if rizin_bin is None:
        return None

    doc = ls.workspace.get_text_document(params.text_document.uri)
    if doc is None:
        return None
    lines = doc.source.split("\n")
    line_idx = params.position.line
    if line_idx >= len(lines):
        return None
    line = lines[line_idx]
    col = params.position.character

    word_match = re.search(r'([a-zA-Z_][a-zA-Z0-9_]*)', line[col:])
    word = word_match.group(1) if word_match else None
    if not word:
        return None

    # Get function address via aflm (list functions matching name)
    output = _run_rizin_cmd(filepath, f"aflm {word}", rizin_bin)
    if not output.strip():
        return None
    try:
        addr_str = output.strip().split()[0]
        addr = int(addr_str, 16)
    except (ValueError, IndexError):
        return None

    return Location(
        uri=params.text_document.uri,
        range=Range(
            start=Position(line=addr // 16, character=0),
            end=Position(line=addr // 16, character=1),
        ),
    )


@SERVER.feature("textDocument/references")
def references(ls, params):
    """Show cross-references to the symbol under cursor."""
    filepath = params.text_document.uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return None

    rizin_bin = _find_rizin()
    if rizin_bin is None:
        return None

    doc = ls.workspace.get_text_document(params.text_document.uri)
    if doc is None:
        return None
    lines = doc.source.split("\n")
    line_idx = params.position.line
    if line_idx >= len(lines):
        return None
    line = lines[line_idx]
    col = params.position.character

    word_match = re.search(r'([a-zA-Z_][a-zA-Z0-9_]*)', line[col:])
    word = word_match.group(1) if word_match else None
    if not word:
        return None

    output = _run_rizin_cmd(filepath, f"axt @ {word}", rizin_bin)
    refs: list[Location] = []
    for line in output.strip().split("\n"):
        if not line.strip():
            continue
        parts = line.strip().split()
        if parts:
            try:
                addr = int(parts[0], 16)
                refs.append(Location(
                    uri=params.text_document.uri,
                    range=Range(
                        start=Position(line=addr // 16, character=0),
                        end=Position(line=addr // 16, character=1),
                    ),
                ))
            except ValueError:
                continue
    return refs if refs else None


if __name__ == "__main__":
    logger.info("Starting rizin-lsp...")
    rizin_bin = _find_rizin()
    if rizin_bin is None:
        logger.warning("rizin binary not found — install via: praetor setup")
    else:
        logger.info("rizin found: %s", rizin_bin)
    SERVER.start_io()