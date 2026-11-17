import io
import json
import os
import struct
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import cli_release as release


class PackagingTests(unittest.TestCase):
    def test_version_and_tag_identity_reject_traversal_and_mismatch(self):
        target = "x86_64-unknown-linux-gnu"
        self.assertEqual(
            release.identity("0.1.0", "a" * 40, target, "cli-v0.1.0"),
            "laby-v0.1.0-" + target,
        )
        for version in ("../evil", "01.2.3", "1.2", "1.2.3\n", "1.2.3/evil"):
            with self.assertRaises(ValueError):
                release.identity(version, "a" * 40, target)
        with self.assertRaises(ValueError):
            release.identity("0.1.0", "a" * 40, target, "cli-v1.0.0")
        with self.assertRaises(ValueError):
            release.identity("0.1.0", "not-a-sha", target)

    def test_lockfile_fingerprint_is_identical_across_checkout_line_endings(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(release, "ROOT", root):
                (root / "Cargo.lock").write_bytes(b"a\r\nb\r\n")
                windows = release.lock_hash()
                (root / "Cargo.lock").write_bytes(b"a\nb\n")
                self.assertEqual(windows, release.lock_hash())
                (root / "Cargo.lock").write_bytes(b"a\nchanged\n")
                self.assertNotEqual(windows, release.lock_hash())

    def test_compiler_evidence_requires_one_successful_complete_build(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "messages.jsonl"
            artifact = {"reason": "compiler-artifact", "package_id": "cli"}
            for finish in (None, False, True):
                messages = [artifact]
                if finish is not None:
                    messages.append({"reason": "build-finished", "success": finish})
                path.write_text(
                    "\n".join(json.dumps(m) for m in messages), encoding="utf-8"
                )
                if finish is True:
                    self.assertEqual(release.compiler_units(path), {"cli"})
                else:
                    with self.assertRaises(ValueError):
                        release.compiler_units(path)

    def test_manifest_uses_compiled_units_and_rejects_unexpected_build_inputs(self):
        packages = [
            {
                "id": name,
                "name": "labyrinth-cli" if name == "cli" else name,
                "version": "1.0.0",
            }
            for name in ("cli", "runtime", "inactive")
        ]
        metadata = {
            "packages": packages,
            "resolve": {
                "nodes": [
                    {
                        "id": "cli",
                        "deps": [
                            {"pkg": name, "dep_kinds": [{"kind": None}]}
                            for name in ("runtime", "inactive")
                        ],
                    },
                    {"id": "runtime", "deps": []},
                    {"id": "inactive", "deps": []},
                ]
            },
        }
        _, selected = release.dependencies(metadata, {"cli", "runtime"})
        self.assertEqual({p["id"] for p in selected}, {"cli", "runtime"})
        for invalid in ({"runtime"}, {"cli", "unknown"}):
            with self.assertRaises(ValueError):
                release.dependencies(metadata, invalid)

    def test_architecture_checks_real_executable_headers(self):
        elf = bytearray(64)
        elf[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", elf, 18, 62)
        release.architecture(elf, "x86_64-unknown-linux-gnu")
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 80)
        pe[80:84] = b"PE\0\0"
        struct.pack_into("<H", pe, 84, 0x8664)
        release.architecture(pe, "x86_64-pc-windows-msvc")
        for target, cpu in [
            ("x86_64-apple-darwin", 0x01000007),
            ("aarch64-apple-darwin", 0x0100000C),
        ]:
            release.architecture(b"\xcf\xfa\xed\xfe" + struct.pack("<I", cpu), target)
        for target in release.TARGETS:
            with self.assertRaises(ValueError):
                release.architecture(b"wrong executable", target)

    def test_archives_are_deterministic_and_keep_executable_mode(self):
        for target in release.TARGETS:
            payload = {
                "laby.exe" if "windows" in target else "laby": b"example",
                "README.md": b"usage",
            }
            first = release.archive(payload, "package", target)
            self.assertEqual(
                first,
                release.archive(
                    dict(reversed(list(payload.items()))), "package", target
                ),
            )
            with tempfile.TemporaryDirectory() as directory:
                file = Path(directory) / ("package." + release.TARGETS[target][2])
                file.write_bytes(first)
                root = release.unpack(file, Path(directory) / "output")
                self.assertEqual((root / "README.md").read_bytes(), b"usage")
                if "windows" not in target:
                    with tarfile.open(file, "r:gz") as bundle:
                        self.assertEqual(bundle.getmember("package/laby").mode, 0o755)
                    if os.name != "nt":
                        self.assertEqual((root / "laby").stat().st_mode & 0o777, 0o755)
                with self.assertRaises(ValueError):
                    release.unpack(file, Path(directory) / "output")

    def test_extraction_rejects_unsafe_and_duplicate_paths_and_links(self):
        for name in (
            "../escape",
            "/absolute/file",
            "root/../escape",
            "C:/drive/file",
            "root\\evil",
            "a/file",
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                file = Path(directory) / "bad.tar.gz"
                with tarfile.open(file, "w:gz") as bundle:
                    info = tarfile.TarInfo(name)
                    info.size = 1
                    bundle.addfile(info, io.BytesIO(b"x"))
                    if name == "a/file":
                        bundle.addfile(info, io.BytesIO(b"x"))
                with self.assertRaises(ValueError):
                    release.unpack(file, Path(directory) / "output")
                self.assertFalse((Path(directory) / "escape").exists())
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "link.tar.gz"
            with tarfile.open(file, "w:gz") as bundle:
                info = tarfile.TarInfo("root/link")
                info.type = tarfile.SYMTYPE
                info.linkname = "/tmp"
                bundle.addfile(info)
            with self.assertRaises(ValueError):
                release.unpack(file, Path(directory) / "output")

    def test_dependency_graph_excludes_dev_only_and_forbids_engine(self):
        metadata = {
            "packages": [
                {"id": "cli", "name": "labyrinth-cli", "version": "0.1.0"},
                {"id": "runtime", "name": "runtime", "version": "1.0.0"},
                {"id": "dev", "name": "test-helper", "version": "1.0.0"},
            ],
            "resolve": {
                "nodes": [
                    {
                        "id": "cli",
                        "deps": [
                            {"pkg": "runtime", "dep_kinds": [{"kind": None}]},
                            {"pkg": "dev", "dep_kinds": [{"kind": "dev"}]},
                        ],
                    },
                    {"id": "runtime", "deps": []},
                    {"id": "dev", "deps": []},
                ]
            },
        }
        _, selected = release.dependencies(metadata)
        self.assertEqual(
            sorted(p["name"] for p in selected), ["labyrinth-cli", "runtime"]
        )
        metadata["packages"][1]["name"] = "labyrinth-engine"
        with self.assertRaises(ValueError):
            release.dependencies(metadata)


if __name__ == "__main__":
    unittest.main()
