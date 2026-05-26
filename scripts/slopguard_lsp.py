#!/usr/bin/env python3
"""slopguard LSP bridge — detects AI-generated code patterns in JS/TS/React.

slopguard is an ESLint plugin (npm). This bridge runs it via npx/eslint
and forwards results as LSP diagnostics.

Current status: slopguard v0.0.1 is a stub (no executable code yet).
Bridge will activate once a real release ships.
"""

import json
import logging
import os
import subprocess
from pathlib import Path

from pygls.lsp.server import LanguageServer
from lsprotocol.types import Diagnostic, DiagnosticSeverity, Position, Range

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("slopguard-lsp")

SERVER = LanguageServer("slopguard-lsp", "v0.1")


def _is_slopguard_usable() -> bool:
    """Check if slopguard has a real CLI entrypoint (v0.0.1 is stub-only)."""
    try:
        result = subprocess.run(
            ["node", "-e", "require('slopguard')"],
            capture_output=True, text=True, timeout=5,
            cwd=os.path.expanduser("~/praetor-lsp"),
        )
        return result.returncode == 0
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False


_USABLE = _is_slopguard_usable()


@SERVER.feature("textDocument/didSave")
def on_save(ls, params):
    uri = params.text_document.uri
    if not _USABLE:
        logger.debug("slopguard not usable (v0.0.1 stub) — skipping")
        return

    filepath = uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return

    try:
        result = subprocess.run(
            ["npx", "--yes", "slopguard", filepath],
            capture_output=True, text=True, timeout=30,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as e:
        logger.warning("slopguard failed: %s", e)
        return

    # slopguard v0.0.1 has no CLI — no output expected.
    # Future versions will produce JSON diagnostics here.
    if result.stdout.strip():
        diags: list[Diagnostic] = []
        for line in result.stdout.split("\n"):
            line = line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            line_num = entry.get("line", 1) - 1
            col = entry.get("column", 0)
            sev = {
                "error": DiagnosticSeverity.Error,
                "warning": DiagnosticSeverity.Warning,
            }.get(entry.get("severity", "warning"), DiagnosticSeverity.Warning)
            diags.append(Diagnostic(
                range=Range(
                    start=Position(line=max(line_num, 0), character=max(col, 0)),
                    end=Position(line=max(line_num, 0), character=max(col + 1, 1)),
                ),
                message=entry.get("message", entry.get("rule", "slopguard")),
                severity=sev,
                source="slopguard",
            ))
        if diags:
            ls.text_document_publish_diagnostics(uri, diags)


if __name__ == "__main__":
    if _USABLE:
        logger.info("starting slopguard-lsp (active)")
    else:
        logger.info("starting slopguard-lsp (stub — waiting for slopguard >0.0.1)")
    SERVER.start_io()