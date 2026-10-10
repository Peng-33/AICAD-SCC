#!/usr/bin/env python3
"""Cross-platform release smoke tests for SCC.

Run against a freshly built release binary:
    python scripts/smoke-test.py

Override the binary path:
    SCC_BIN=./target/release/scc python scripts/smoke-test.py

The script returns a non-zero exit code on the first failure and prints the
checkpoint name together with the actual value.
"""

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = REPO_ROOT / "examples"
EXPECTED_VERSION = "scc 0.4.0-alpha.1"


def fail(checkpoint: str, actual: str) -> None:
    print(f"FAIL: {checkpoint}")
    print(f"  actual: {actual}")
    sys.exit(1)


def find_scc_bin() -> Path:
    env = os.environ.get("SCC_BIN")
    if env:
        return Path(env)

    name = "scc.exe" if platform.system() == "Windows" else "scc"
    return REPO_ROOT / "target" / "release" / name


def run(cmd: list[str | Path], *, expect_success: bool = True) -> subprocess.CompletedProcess:
    result = subprocess.run(
        [str(c) for c in cmd],
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    if expect_success and result.returncode != 0:
        fail(
            f"command failed: {' '.join(str(c) for c in cmd)}",
            f"exit={result.returncode} stderr={result.stderr.strip()}",
        )
    if not expect_success and result.returncode == 0:
        fail(
            f"command unexpectedly succeeded: {' '.join(str(c) for c in cmd)}",
            f"stdout={result.stdout.strip()}",
        )
    return result


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    scc = find_scc_bin()
    if not scc.exists():
        fail("SCC binary exists", str(scc))

    with tempfile.TemporaryDirectory(prefix="scc-smoke-") as tmp:
        tmp_dir = Path(tmp)
        ir_first = tmp_dir / "bracket.ir.json"
        ir_second = tmp_dir / "bracket.ir.2.json"
        trace_first = tmp_dir / "bracket.trace.json"
        trace_second = tmp_dir / "bracket.trace.2.json"
        conflict_report = tmp_dir / "bracket.conflict.json"
        geometry_output = tmp_dir / "fixed-circle.geometry.json"
        svg_output = tmp_dir / "fixed-circle.svg"
        dxf_output = tmp_dir / "fixed-circle.dxf"

        # Checkpoint 1: version consistency
        version_result = run([scc, "--version"])
        version = version_result.stdout.strip()
        if version != EXPECTED_VERSION:
            fail("scc --version", version)

        # Checkpoint 2: compile valid proposal
        run([scc, "compile", EXAMPLES / "bracket.proposal.json", "--output", ir_first])
        if not ir_first.exists():
            fail("compile creates IR file", "file missing")
        with open(ir_first, "r", encoding="utf-8") as f:
            ir_data = json.load(f)
        if not ir_data.get("entities"):
            fail("IR contains entities", "entities missing or empty")

        # Checkpoint 3: compile determinism
        run([scc, "compile", EXAMPLES / "bracket.proposal.json", "--output", ir_second, "--force"])
        if sha256_file(ir_first) != sha256_file(ir_second):
            fail("compile determinism", "IR hashes differ")

        # Checkpoint 4: overwrite protection
        run([scc, "compile", EXAMPLES / "bracket.proposal.json", "--output", ir_second], expect_success=False)
        if not ir_second.exists():
            fail("overwrite protection preserves file", "file missing after failed overwrite")

        # Checkpoint 5: force overwrite
        run([scc, "compile", EXAMPLES / "bracket.proposal.json", "--output", ir_second, "--force"])
        with open(ir_second, "r", encoding="utf-8") as f:
            json.load(f)  # validate JSON is not truncated

        # Checkpoint 6: execute IR
        run([scc, "execute", ir_first, "--trace", trace_first])
        state_first = trace_first.with_suffix(".state.json")
        if not trace_first.exists():
            fail("execute creates trace file", "trace missing")
        if not state_first.exists():
            fail("execute creates state file", "state missing")
        with open(trace_first, "r", encoding="utf-8") as f:
            trace_data = json.load(f)
        with open(state_first, "r", encoding="utf-8") as f:
            state_data = json.load(f)
        if not state_data.get("entities"):
            fail("GraphState contains entities", "entities missing or empty")

        # Checkpoint 7: execute determinism
        run([scc, "execute", ir_first, "--trace", trace_second, "--force"])
        state_second = trace_second.with_suffix(".state.json")
        if sha256_file(trace_first) != sha256_file(trace_second):
            fail("execute trace determinism", "trace hashes differ")
        if sha256_file(state_first) != sha256_file(state_second):
            fail("execute state determinism", "state hashes differ")

        # Checkpoint 8: conflict proposal produces conflict report
        run([scc, "compile", EXAMPLES / "bracket.conflict.proposal.json", "--output", conflict_report])
        with open(conflict_report, "r", encoding="utf-8") as f:
            report = json.load(f)
        if report.get("status") != "conflict":
            fail("conflict report status", report.get("status"))
        if not report.get("conflicts"):
            fail("conflict report contains conflicts", "conflicts missing or empty")

        # Checkpoint 9: solve GraphState into GeometryModel
        run(
            [
                scc,
                "solve",
                EXAMPLES / "fixed-circle.state.json",
                "--output",
                geometry_output,
            ]
        )
        with open(geometry_output, "r", encoding="utf-8") as f:
            geometry = json.load(f)
        radius = (
            geometry.get("entities", {})
            .get("circle", {})
            .get("primitive", {})
            .get("radius")
        )
        if radius != 5.0:
            fail("solve fixed circle radius", str(radius))

        # Checkpoint 10: export solved geometry to both CAD subsets
        run([scc, "export", geometry_output, "--format", "svg", "--output", svg_output])
        run([scc, "export", geometry_output, "--format", "dxf", "--output", dxf_output])
        if not svg_output.read_text(encoding="utf-8").startswith("<svg "):
            fail("SVG export header", "missing SVG root")
        if not dxf_output.read_text(encoding="utf-8").startswith("0\nSECTION\n2\nHEADER\n"):
            fail("DXF export header", "missing DXF header section")

        # Checkpoint 11: no temporary files left behind
        temp_files = [p for p in tmp_dir.iterdir() if p.name.endswith(".tmp")]
        if temp_files:
            fail("no temporary files left", str(temp_files))

    print("All smoke tests passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
