"""Audit the exact dependency union embedded in all four native CLI packages."""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

import cli_release as package


def scope(directory):
    found = set()
    for target, (_, _, extension) in package.TARGETS.items():
        matches = list(directory.glob(f"laby-v*-{target}.{extension}"))
        if len(matches) != 1:
            raise ValueError(
                "One native archive per target required for dependency audit"
            )
        with tempfile.TemporaryDirectory() as temporary:
            root = package.unpack(matches[0], Path(temporary) / "package")
            entries = json.loads(
                (root / "THIRD_PARTY_NOTICES.json").read_text(encoding="utf-8")
            )
            found.update((entry["name"], entry["version"]) for entry in entries)
    return found


def classify(report, dependencies):
    blocking, review, outside = [], [], []
    if (
        "database" not in report
        or "vulnerabilities" not in report
        or "warnings" not in report
    ):
        raise ValueError("Incomplete audit report")
    findings = [("vulnerability", v) for v in report["vulnerabilities"].get("list", [])]
    findings.extend(
        (kind, v) for kind, values in report["warnings"].items() for v in values
    )
    for kind, value in findings:
        dependency = value["package"]
        advisory = value.get("advisory") or {}
        entry = {
            "name": dependency["name"],
            "version": dependency["version"],
            "kind": kind,
            "advisory": advisory.get("id"),
            "title": advisory.get("title"),
        }
        if (entry["name"], entry["version"]) not in dependencies:
            outside.append(entry)
        elif kind == "unmaintained":
            review.append(entry)
        else:
            blocking.append(entry)
    return blocking, review, outside


def run(directory, output, tool, revision):
    dependencies = scope(directory)
    output.mkdir(parents=True, exist_ok=True)
    command = [
        str(tool),
        "audit",
        "--json",
        "--file",
        str(package.ROOT / "Cargo.lock"),
        "--deny",
        "unsound",
        "--deny",
        "yanked",
    ]
    result = subprocess.run(
        command, capture_output=True, text=True, timeout=180, check=False
    )
    if result.returncode not in (0, 1) or not result.stdout:
        raise ValueError("Dependency audit failed; no stale/offline fallback is used")
    report = json.loads(result.stdout)
    blocking, review, outside = classify(report, dependencies)
    if result.returncode == 1 and not blocking and not outside and not review:
        raise ValueError("Audit failed without a complete advisory report")
    (output / "cargo-audit.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    summary = {
        "schema_version": "cli_dependency_audit_v1",
        "revision": revision,
        "cargo_lock_sha256": package.sha((package.ROOT / "Cargo.lock").read_bytes()),
        "tool": "cargo-audit 0.23.0",
        "database": report["database"],
        "targets": sorted(package.TARGETS),
        "dependencies": [
            {"name": name, "version": version} for name, version in sorted(dependencies)
        ],
        "blocking": blocking,
        "review": review,
        "outside_cli_scope": outside,
    }
    (output / "dependency-audit.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if blocking:
        raise ValueError(
            "CLI dependency advisory/yank blocks release; inspect the audit report"
        )
    print(
        f"CLI dependency audit passed ({len(dependencies)} dependency versions; {len(review)} maintenance warnings for review)"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--tool", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    run(args.directory, args.output, args.tool, args.revision)
