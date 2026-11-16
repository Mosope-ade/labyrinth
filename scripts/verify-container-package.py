#!/usr/bin/env python3
"""Verify an offline engine-child wire package against its public fixture input."""
import hashlib
import json
import pathlib
import sys

NAMES = {
    "bom.json", "catalog.json", "decisions.json", "firewall.json", "ipam.json",
    "manifest.json", "options.json", "provenance.json", "requirements.json",
    "routing.json", "selection.json", "topology.json", "verification.json",
}


def verify(fixture_path, wire_path):
    fixture = json.loads(pathlib.Path(fixture_path).read_bytes())
    with open(wire_path,"rb") as source:
        wire_bytes = source.read(128 * 1024 * 1024 + 1)
    if len(wire_bytes) > 128 * 1024 * 1024:
        raise ValueError("wire output exceeds bound")
    wire = json.loads(wire_bytes)
    if set(wire) != NAMES:
        raise ValueError("unexpected inventory")
    files = {name: bytes(value) for name, value in wire.items()}
    if sum(map(len, files.values())) > 32 * 1024 * 1024:
        raise ValueError("decoded package exceeds bound")
    manifest = json.loads(files["manifest.json"])
    if manifest["schema_version"] != "engineering_package_v1":
        raise ValueError("unsupported package schema")
    if set(manifest["files"]) != NAMES - {"manifest.json"}:
        raise ValueError("invalid manifest inventory")
    for name, expected in manifest["files"].items():
        if expected["sha256"] != hashlib.sha256(files[name]).hexdigest():
            raise ValueError("payload digest mismatch")
        if expected["size_bytes"] != len(files[name]):
            raise ValueError("payload size mismatch")
    context = fixture["context"]
    if manifest["catalog_version"]["sha256"] != context["catalog_sha256"]:
        raise ValueError("catalog identity mismatch")
    for name in ["requirements", "provenance", "options"]:
        if json.loads(files[name + ".json"]) != context[name]:
            raise ValueError("input identity mismatch")
    for name in ["requirements", "options"]:
        if manifest[name + "_sha256"] != hashlib.sha256(files[name + ".json"]).hexdigest():
            raise ValueError("manifest input hash mismatch")
    report = json.loads(files["verification.json"])
    if len(report["passes"]) != 5 or not all(p["passed"] is True for p in report["passes"]):
        raise ValueError("five-pass verification did not complete")
    if {p["kind"] for p in report["passes"]} != {"structural","addressing","routing","security","resilience"}:
        raise ValueError("invalid verification pass inventory")
    print("Verified offline 13-artifact container fixture package")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        raise SystemExit("usage: verify-container-package.py FIXTURE WIRE_OUTPUT")
    verify(sys.argv[1], sys.argv[2])
