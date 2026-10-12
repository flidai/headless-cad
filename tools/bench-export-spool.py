#!/usr/bin/env python3
"""Compare CLI export retention against an earlier binary on the same recipe.

This measures the whole cad process, including the kernel, mesh generation and
one-at-a-time exporter allocations. The intended signal is the peak resident
memory removed by no longer retaining every completed export as a String.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time


def source(parts: int) -> str:
    # Distinct geometry prevents the build and mesh caches from collapsing the
    # per-output exports into a single shared component.
    lines = ["# Independent circular stock, each with distinct exact dimensions.\n"]
    for i in range(parts):
        lines.append(
            f"p{i} = circle(id = 'p{i}', radius = {2 + i / 1000:.3f})\n"
            f"b{i} = extrude(id = 'b{i}', profile = p{i}, height = {3 + i / 1000:.3f})\n"
            f"output(name = 'part_{i:04d}', feature = b{i})\n"
        )
    return "".join(lines)


def resident_kib(pid: int) -> tuple[int, int]:
    try:
        fields = Path(f"/proc/{pid}/status").read_text().splitlines()
    except (FileNotFoundError, ProcessLookupError):
        return 0, 0
    values = {}
    for line in fields:
        if line.startswith(("VmRSS:", "VmHWM:")):
            key, value, *_ = line.split()
            values[key.removesuffix(":")] = int(value)
    return values.get("VmRSS", 0), values.get("VmHWM", 0)


def inventory(out: Path) -> dict[str, str]:
    return {
        file.name: hashlib.sha256(file.read_bytes()).hexdigest()
        for file in sorted(out.iterdir())
        if file.is_file()
    }


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def semantic_manifest(out: Path) -> dict:
    manifest = json.loads((out / "manifest.json").read_text())
    # These are the source and analytic results users act on. Publication hashes
    # and execution counters necessarily change when a new exporter build is
    # introduced, even if the underlying part geometry is identical.
    keys = (
        "schema_version", "units", "tolerance", "scope", "outputs",
        "face_assertions", "property_assertions",
        "clearance_checks", "section_views", "datum_dimensions",
        "tolerance_stacks", "rigid_rotation_groups", "artifact_files",
        "recipe_source_hash", "recipe_source", "generation_mode",
    )
    semantic = {key: manifest[key] for key in keys if key in manifest}
    # Feature and output content hashes deliberately include the kernel build
    # identity. They change on any kernel source edit even when exported bytes
    # and measured topology stay exactly the same.
    for output in semantic.get("outputs", {}).values():
        output.pop("content_hash", None)
    return semantic


def semantic_exchange(out: Path, name: str) -> dict:
    report = json.loads((out / name).read_text())
    report.pop("recipe_context_hash", None)
    report.pop("source_content_hash", None)
    return report


def run(binary: Path, recipe: Path, out: Path) -> dict:
    with tempfile.TemporaryFile() as error:
        start = time.monotonic()
        process = subprocess.Popen(
            [str(binary), "build", str(recipe), "--out-dir", str(out)],
            stdout=subprocess.DEVNULL,
            stderr=error,
            env={key: value for key, value in os.environ.items() if key != "CAD_CACHE_DIR"},
        )
        rss_peak = hwm_peak = 0
        while process.poll() is None:
            rss, hwm = resident_kib(process.pid)
            rss_peak = max(rss_peak, rss)
            hwm_peak = max(hwm_peak, hwm)
            time.sleep(0.02)
        elapsed = time.monotonic() - start
        if process.returncode:
            error.seek(0)
            raise RuntimeError(f"{binary} failed: {error.read().decode(errors='replace')}")
    files = inventory(out)
    manifest = json.loads((out / "manifest.json").read_text())
    verified = subprocess.run(
        [str(binary), "verify", str(out)],
        capture_output=True,
        check=True,
        text=True,
    )
    verification = json.loads(verified.stdout)
    if verification.get("valid") is not True or verification.get("verified_files") != len(files) - 1:
        raise RuntimeError(f"{binary} did not verify its publication: {verification}")
    return {
        "elapsed_seconds": round(elapsed, 3),
        "peak_rss_kib": max(rss_peak, hwm_peak),
        "artifacts": len(files),
        "artifact_total_bytes": sum((out / name).stat().st_size for name in files),
        "largest_artifact_bytes": max((out / name).stat().st_size for name in files),
        "manifest_artifacts": len(manifest["artifact_files"]),
        "verification": {
            "valid": verification["valid"],
            "verified_files": verification["verified_files"],
            "verified_bytes": verification["verified_bytes"],
        },
        "hashes": files,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path)
    parser.add_argument("--parts", type=int, default=256)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if not 1 <= args.parts <= 999:
        parser.error("--parts must be in 1..999")
    temporary = None
    if args.work_dir is None:
        temporary = tempfile.TemporaryDirectory(prefix="cad-export-spool-")
        work = Path(temporary.name)
    else:
        work = args.work_dir.resolve()
        work.mkdir(parents=True, exist_ok=True)
    recipe = work / "export_spool.star"
    recipe.write_text(source(args.parts))
    before_binary = args.before.resolve()
    before = run(before_binary, recipe, work / "before")
    report = {
        "memory_measurement": "maximum observed /proc/<pid>/status VmHWM or VmRSS, polled every 20 ms",
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
        },
        "binary_sha256": {"before": file_sha256(before_binary)},
        "fixture": {"parts": args.parts, "source_sha256": hashlib.sha256(recipe.read_bytes()).hexdigest()},
        "before": before,
    }
    if args.after:
        after_binary = args.after.resolve()
        after = run(after_binary, recipe, work / "after")
        report["binary_sha256"]["after"] = file_sha256(after_binary)
        common = sorted(set(before["hashes"]) & set(after["hashes"]))
        mismatch = [name for name in common if before["hashes"][name] != after["hashes"][name]]
        geometry = [name for name in common if name.endswith((".step", ".stl", ".brep.json"))]
        exchange = [name for name in common if name.endswith(".exchange.json")]
        report["after"] = after
        report["comparison"] = {
            "all_filenames_equal": before["hashes"].keys() == after["hashes"].keys(),
            "exact_geometry_bytes_equal": all(name not in mismatch for name in geometry),
            "exact_geometry_files": len(geometry),
            "semantic_manifest_equal": semantic_manifest(work / "before") == semantic_manifest(work / "after"),
            "semantic_exchange_reports_equal": all(
                semantic_exchange(work / "before", name) == semantic_exchange(work / "after", name)
                for name in exchange
            ),
            "exchange_reports": len(exchange),
            "mismatched_artifacts": mismatch,
            "peak_rss_reduction_kib": before["peak_rss_kib"] - after["peak_rss_kib"],
        }
        comparison = report["comparison"]
        comparison["accepted"] = all((
            comparison["all_filenames_equal"],
            comparison["exact_geometry_bytes_equal"],
            comparison["exact_geometry_files"] == 3 * args.parts,
            comparison["semantic_manifest_equal"],
            comparison["semantic_exchange_reports_equal"],
            comparison["exchange_reports"] == args.parts,
            before["artifacts"] == before["manifest_artifacts"],
            after["artifacts"] == after["manifest_artifacts"],
        ))
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    # Avoid dumping thousands of hashes into a terminal, while the report file
    # retains the full byte inventory for independent review.
    shown = {
        key: {k: v for k, v in value.items() if k != "hashes"}
        if key in ("before", "after") else value.copy() if key == "comparison" else value
        for key, value in report.items()
    }
    if "comparison" in shown:
        mismatched = shown["comparison"].pop("mismatched_artifacts")
        shown["comparison"]["mismatched_artifact_count"] = len(mismatched)
        shown["comparison"]["mismatched_artifact_examples"] = mismatched[:6]
    print(json.dumps(shown, indent=2, sort_keys=True))
    if args.after and not report["comparison"]["accepted"]:
        raise SystemExit("export spool benchmark parity checks failed")
    if temporary:
        temporary.cleanup()


if __name__ == "__main__":
    main()
