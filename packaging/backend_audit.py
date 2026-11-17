"""Audit compiler-selected backend dependencies; retain inactive lockfile findings."""

import argparse
import json
import subprocess
from pathlib import Path

import cli_audit
import cli_release as package

COMPONENTS = {"labyrinth-api", "labyrinth-worker", "labyrinth-engine"}


def scope(metadata, units):
    packages = {entry["id"]: entry for entry in metadata["packages"]}
    if not units or not units <= packages.keys():
        raise ValueError("Compiler inventory has missing or unknown package IDs")
    selected = [packages[identifier] for identifier in units]
    if not COMPONENTS <= {entry["name"] for entry in selected}:
        raise ValueError("Compiler inventory does not cover every backend component")
    return {
        (entry["name"], entry["version"])
        for entry in selected
        if entry["source"] is not None
    }


def run(output, tool):
    output.mkdir(parents=True, exist_ok=False)
    lock_hash = package.lock_hash()
    command = ["cargo", "build", "--locked", "--release", "--bins", "--lib"]
    for component in sorted(COMPONENTS):
        command.extend(["-p", component])
    command.append("--message-format=json-render-diagnostics")
    units_path = output / "compiler-units.jsonl"
    with units_path.open("w", encoding="utf-8") as messages:
        subprocess.run(
            command, cwd=package.ROOT, stdout=messages, check=True, timeout=1200
        )
    # These examples are installed as executables by deploy/Dockerfile.
    maintenance_path = output / "maintenance-units.jsonl"
    with maintenance_path.open("w", encoding="utf-8") as messages:
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--release",
                "-p",
                "labyrinth-api",
                "--example",
                "migrate",
                "--example",
                "prune_audit",
                "--message-format=json-render-diagnostics",
            ],
            cwd=package.ROOT,
            stdout=messages,
            check=True,
            timeout=1200,
        )
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version=1", "--locked"],
            cwd=package.ROOT,
            timeout=180,
        )
    )
    dependencies = scope(
        metadata,
        package.compiler_units(units_path) | package.compiler_units(maintenance_path),
    )
    result = subprocess.run(
        [
            str(tool),
            "audit",
            "--json",
            "--file",
            str(package.ROOT / "Cargo.lock"),
            "--deny",
            "unsound",
            "--deny",
            "yanked",
        ],
        cwd=package.ROOT,
        capture_output=True,
        text=True,
        timeout=180,
        check=False,
    )
    if result.returncode not in (0, 1) or not result.stdout:
        raise ValueError("Audit failed; no stale/offline fallback is used")
    report = json.loads(result.stdout)
    blocking, review, outside = cli_audit.classify(report, dependencies)
    if result.returncode == 1 and not (blocking or review or outside):
        raise ValueError("Audit failed without complete findings")
    if lock_hash != package.lock_hash():
        raise ValueError("Lockfile changed while collecting audit evidence")
    (output / "cargo-audit.json").write_text(json.dumps(report, indent=2) + "\n")
    summary = {
        "schema_version": "backend_dependency_audit_v1",
        "cargo_lock_sha256": lock_hash,
        "database": report["database"],
        "components": sorted(COMPONENTS),
        "dependencies": [{"name": n, "version": v} for n, v in sorted(dependencies)],
        "blocking": blocking,
        "maintenance_review": review,
        "outside_backend_scope": outside,
    }
    (output / "dependency-audit.json").write_text(json.dumps(summary, indent=2) + "\n")
    if blocking or review:
        raise ValueError("Backend advisory/maintenance finding requires remediation")
    print(f"Backend audit passed ({len(dependencies)} compiled dependency versions)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tool", type=Path, required=True)
    args = parser.parse_args()
    run(args.output, args.tool.resolve())
