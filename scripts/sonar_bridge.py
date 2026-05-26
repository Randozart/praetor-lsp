#!/usr/bin/env python3
"""TCP bridge for SonarLint language server (JAR mode).

SonarLint v4.x JAR acts as a TCP client: it connects BACK to the port
passed as argument. This script binds a local port, spawns the JAR
pointing at that port, and forwards LSP stdio ↔ TCP.
"""

import logging
import os
import socket
import subprocess
import sys
import threading

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger("sonar-bridge")

JAVA_BIN = os.environ.get("SONAR_JAVA", "/home/randozart/.local/opt/java17/bin/java")
JAR_PATH = os.environ.get("SONAR_JAR",
    os.path.expanduser("~/praetor-lsp/lib/sonarlint-language-server.jar"))


def _forward(src, dst):
    try:
        while True:
            data = src.read(4096)
            if not data:
                break
            dst.write(data)
            dst.flush()
    except (BrokenPipeError, ConnectionError, OSError):
        pass


def _drain_stderr(proc):
    for line in proc.stderr:
        if line:
            logger.warning("jar stderr: %s", line.decode().strip())


def main():
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 0))
    port = server.getsockname()[1]
    server.listen(1)

    logger.info("listening on port %d, starting SonarLint JAR...", port)

    jar_proc = subprocess.Popen(
        [JAVA_BIN, "-jar", JAR_PATH, str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    threading.Thread(target=_drain_stderr, args=(jar_proc,), daemon=True).start()

    server.settimeout(10)
    try:
        conn, addr = server.accept()
    except socket.timeout:
        logger.error("SonarLint JAR did not connect within 10s")
        jar_proc.kill()
        sys.exit(1)
    finally:
        server.close()

    logger.info("SonarLint connected from %s", addr)

    t1 = threading.Thread(target=_forward, args=(sys.stdin.buffer, conn.makefile("wb")), daemon=True)
    t2 = threading.Thread(target=_forward, args=(conn.makefile("rb"), sys.stdout.buffer), daemon=True)
    t1.start()
    t2.start()
    t1.join()

    jar_proc.wait()


if __name__ == "__main__":
    main()