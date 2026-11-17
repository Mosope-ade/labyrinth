import json
import struct
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import cli_publish as publish
import cli_release as release

REVISION = "a" * 40


def artifacts(directory, tag=None):
    for target, (_, _, extension) in release.TARGETS.items():
        stem = release.identity("0.1.0", REVISION, target, tag)
        if "linux" in target:
            data = bytearray(64)
            data[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<H", data, 18, 62)
        elif "windows" in target:
            data = bytearray(128)
            data[:2] = b"MZ"
            struct.pack_into("<I", data, 60, 80)
            data[80:84] = b"PE\0\0"
            struct.pack_into("<H", data, 84, 0x8664)
        else:
            data = b"\xcf\xfa\xed\xfe" + struct.pack(
                "<I", 0x0100000C if target.startswith("aarch64") else 0x01000007
            )
        payload = {
            "laby.exe" if "windows" in target else "laby": bytes(data),
            "README.md": b"test instructions",
            "LICENSE": b"synthetic test license",
            "BUILD.json": json.dumps(
                {
                    "schema_version": "cli_package_v1",
                    "version": "0.1.0",
                    "revision": REVISION,
                    "target": target,
                    "tag": tag,
                    "executable_sha256": release.sha(data),
                    "cargo_lock_sha256": release.lock_hash(),
                    "project_license_included": True,
                }
            ).encode(),
        }
        payload["FILES.sha256"] = "".join(
            f"{release.sha(data)}  {name}\n" for name, data in sorted(payload.items())
        ).encode()
        archive = directory / f"{stem}.{extension}"
        archive.write_bytes(release.archive(payload, stem, target))
        archive.with_name(archive.name + ".sha256").write_text(
            f"{release.sha(archive.read_bytes())}  {archive.name}\n", encoding="ascii"
        )


class PublicationTests(unittest.TestCase):
    def test_complete_set_generates_only_expected_release_assets(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            artifacts(directory)
            assets = publish.assemble(directory, "0.1.0", REVISION)
            self.assertEqual(len(assets), 6)
            self.assertEqual(
                len((directory / "SHA256SUMS").read_text().splitlines()), 4
            )
            self.assertEqual(
                json.loads((directory / "release-index.json").read_text())["revision"],
                REVISION,
            )

    def test_corrupt_missing_unexpected_or_wrong_provenance_set_fails(self):
        for case in ("corrupt", "missing", "unexpected", "wrong_revision"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp)
                artifacts(directory)
                first = min(directory.glob("*.tar.gz"))
                if case == "corrupt":
                    first.write_bytes(b"corrupt")
                elif case == "missing":
                    first.unlink()
                elif case == "unexpected":
                    (directory / ".env").write_text("should never ship")
                with self.assertRaises((ValueError, FileNotFoundError)):
                    publish.assemble(
                        directory,
                        "0.1.0",
                        "b" * 40 if case == "wrong_revision" else REVISION,
                    )
                self.assertFalse((directory / "SHA256SUMS").exists())

    def test_release_creation_is_draft_only_and_confirms_uploads(self):
        calls = []

        def request(
            url, token, body=None, content_type="application/json", method=None
        ):
            calls.append((url, body))
            self.assertEqual(token, "test-token")
            if "/releases?" in url:
                return []
            if "/git/ref/" in url:
                return {"object": {"type": "commit", "sha": REVISION}}
            if url.endswith("/releases"):
                data = json.loads(body)
                self.assertTrue(data["draft"])
                self.assertTrue(data["prerelease"])
                self.assertEqual(data["target_commitish"], REVISION)
                return {"id": 1}
            return {
                "name": "SHA256SUMS",
                "size": len(body),
                "state": "uploaded",
                "digest": "sha256:" + release.sha(body),
            }

        with tempfile.TemporaryDirectory() as temp:
            asset = Path(temp) / "SHA256SUMS"
            asset.write_bytes(b"test checksums")
            with (
                patch.dict("os.environ", {"GH_TOKEN": "test-token"}),
                patch.object(publish, "request", request),
                patch("builtins.print"),
            ):
                publish.publish("owner/repo", "cli-v0.1.0", REVISION, [asset])
        self.assertEqual(len(calls), 4)

    def test_existing_draft_or_moved_tag_prevents_mutations(self):
        for case in ("draft", "moved"):
            calls = []

            def request(url, token, body=None, _calls=calls, _case=case, **kwargs):
                _calls.append(body)
                if "/releases?" in url:
                    return (
                        [{"tag_name": "cli-v0.1.0", "draft": True}]
                        if _case == "draft"
                        else []
                    )
                return {"object": {"type": "commit", "sha": "b" * 40}}

            with (
                patch.dict("os.environ", {"GH_TOKEN": "test-token"}),
                patch.object(publish, "request", request),
                self.assertRaises(ValueError),
            ):
                publish.publish("owner/repo", "cli-v0.1.0", REVISION, [])
            self.assertTrue(all(body is None for body in calls))


if __name__ == "__main__":
    unittest.main()
