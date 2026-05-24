#!/usr/bin/env python3
"""LSP bridge for Meta's Infer formal verification tool.

Runs `infer --pulse-only` on save and forwards diagnostics to OpenCode.
"""

import json
import os
import subprocess
import tempfile
from pathlib import Path

from pygls.lsp.server import LanguageServer
from lsprotocol.types import Diagnostic, DiagnosticSeverity, Position, Range

SERVER = LanguageServer("infer-lsp", "v0.1")

COMPILER_MAP = {
    ".c":   ["gcc", "-c"],
    ".cpp": ["g++", "-c"],
    ".cc":  ["g++", "-c"],
    ".cxx": ["g++", "-c"],
    ".h":   ["gcc", "-c"],
    ".hpp": ["g++", "-c"],
    ".java": ["javac"],
    ".cs":   ["mcs"],
    ".m":    ["clang", "-c"],
    ".mm":   ["clang++", "-c"],
}

SEVERITY_MAP = {
    "ERROR": DiagnosticSeverity.Error,
    "WARNING": DiagnosticSeverity.Warning,
    "INFO": DiagnosticSeverity.Information,
}


def _run_infer(filepath: str) -> list[Diagnostic]:
    ext = Path(filepath).suffix.lower()
    compiler = COMPILER_MAP.get(ext)
    if compiler is None:
        return []

    workdir = Path(filepath).parent
    infer_out = workdir / "infer-out"
    report_path = infer_out / "report.json"

    if infer_out.exists():
        subprocess.run(["rm", "-rf", str(infer_out)], capture_output=True)

    cmd = ["infer", "--pulse-only", "--"] + compiler + [filepath]
    try:
        subprocess.run(cmd, cwd=str(workdir), capture_output=True, timeout=120)
    except subprocess.TimeoutExpired:
        return []

    if not report_path.exists():
        return []

    with open(report_path) as f:
        try:
            report = json.load(f)
        except json.JSONDecodeError:
            return []

    diags: list[Diagnostic] = []
    bugs = report if isinstance(report, list) else report.get("bugs", [])
    for bug in bugs:
        line = bug.get("line", 1) - 1
        col = bug.get("column", 1) - 1
        sev = SEVERITY_MAP.get(bug.get("severity", "WARNING"), DiagnosticSeverity.Warning)
        msg = f"[{bug.get('bug_type', 'INFER')}] {bug.get('qualifier', '')}"
        diags.append(Diagnostic(
            range=Range(
                start=Position(line=line, character=max(col, 0)),
                end=Position(line=line, character=max(col + 1, 1)),
            ),
            message=msg,
            severity=sev,
            source="infer",
        ))

    return diags


@SERVER.feature("textDocument/didSave")
def on_save(ls, params):
    uri = params.text_document.uri
    filepath = uri.replace("file://", "")
    if not os.path.isfile(filepath):
        return
    diags = _run_infer(filepath)
    ls.text_document_publish_diagnostics(uri, diags)


if __name__ == "__main__":
    SERVER.start_io()
