"""Assemble verified native artifacts; optionally create a NEW draft prerelease."""

import argparse
import json
import os
import re
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import cli_release as package


def assemble(directory, version, revision, tag=None):
    index, checksums = [], []
    expected = set()
    for target, (_, _, extension) in package.TARGETS.items():
        stem = package.identity(version, revision, target, tag)
        asset = directory / f"{stem}.{extension}"
        sidecar = asset.with_name(asset.name + ".sha256")
        expected.update((asset.name, sidecar.name))
        checksum = package.sha(asset.read_bytes())
        if sidecar.read_text(encoding="ascii") != f"{checksum}  {asset.name}\n":
            raise ValueError("Archive checksum mismatch")
        with tempfile.TemporaryDirectory(prefix="laby release verify ") as temporary:
            root = package.unpack(asset, Path(temporary) / "package")
            info, _ = package.verify_tree(root, target, version, revision)
            if (info["version"], info["revision"], info["target"], info["tag"]) != (
                version,
                revision,
                target,
                tag,
            ):
                raise ValueError("Cross-platform artifact provenance mismatch")
            if tag and (
                not info["project_license_included"] or not (root / "LICENSE").is_file()
            ):
                raise ValueError("Project license required before release")
            executable = root / ("laby.exe" if "windows" in target else "laby")
            package.architecture(executable.read_bytes(), target)
            if package.sha(executable.read_bytes()) != info["executable_sha256"]:
                raise ValueError("Executable integrity mismatch")
        index.append(
            {
                "name": asset.name,
                "sha256": checksum,
                "size_bytes": asset.stat().st_size,
                "target": target,
            }
        )
        checksums.append(f"{checksum}  {asset.name}\n")
    if {p.name for p in directory.iterdir()} != expected:
        raise ValueError("Unexpected files in the release asset set")
    (directory / "SHA256SUMS").write_text(
        "".join(sorted(checksums)), encoding="ascii", newline="\n"
    )
    (directory / "release-index.json").write_text(
        json.dumps(
            {
                "schema_version": "cli_release_v1",
                "version": version,
                "revision": revision,
                "tag": tag,
                "assets": index,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return [directory / x["name"] for x in index] + [
        directory / "SHA256SUMS",
        directory / "release-index.json",
    ]


def request(url, token, body=None, content_type="application/json", method=None):
    req = urllib.request.Request(
        url,
        data=body,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "Authorization": f"Bearer {token}",
            "User-Agent": "labyrinth-cli-release",
            "Content-Type": content_type,
        },
    )

    # Never send the release token to a redirected host.
    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *args, **kwargs):
            return None

    with urllib.request.build_opener(NoRedirect).open(req, timeout=60) as response:
        return json.loads(response.read(1024 * 1024))


def publish(repository, tag, revision, assets):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Invalid repository")
    token = os.environ.get("GH_TOKEN")
    if not token:
        raise ValueError("Missing narrowly scoped release-job token")
    base = f"https://api.github.com/repos/{repository}"
    # Authenticated listing includes drafts; tag lookup alone can miss an old draft.
    for page in range(1, 21):
        releases = request(f"{base}/releases?per_page=100&page={page}", token)
        if not isinstance(releases, list):
            raise TypeError("Invalid release preflight response")
        if any(release.get("tag_name") == tag for release in releases):
            raise ValueError(
                "Release tag already exists; never overwrite or repair automatically"
            )
        if len(releases) < 100:
            break
    else:
        raise ValueError("Release preflight page bound reached; inspect manually")
    reference = request(
        f"{base}/git/ref/tags/{urllib.parse.quote(tag, safe='')}", token
    )["object"]
    for _ in range(4):
        if reference.get("type") == "commit":
            break
        if reference.get("type") != "tag" or not re.fullmatch(
            r"[0-9a-f]{40}", reference.get("sha", "")
        ):
            raise ValueError("Invalid release tag reference")
        reference = request(f"{base}/git/tags/{reference['sha']}", token)["object"]
    if reference.get("type") != "commit" or reference.get("sha") != revision:
        raise ValueError(
            "Release tag moved or does not identify the verified source commit"
        )
    notes = (Path(__file__).parent / "cli/RELEASE_NOTES.md").read_text(encoding="utf-8")
    body = json.dumps(
        {
            "tag_name": tag,
            "target_commitish": revision,
            "name": f"Labyrinth CLI {tag}",
            "body": notes,
            "draft": True,
            "prerelease": True,
            "generate_release_notes": False,
        }
    ).encode()
    created = request(f"{base}/releases", token, body)
    release_id = created["id"]
    for asset in assets:
        url = (
            f"https://uploads.github.com/repos/{repository}/releases/{release_id}/assets?"
            + urllib.parse.urlencode({"name": asset.name})
        )
        uploaded = request(url, token, asset.read_bytes(), "application/octet-stream")
        if (
            uploaded.get("name") != asset.name
            or uploaded.get("size") != asset.stat().st_size
            or uploaded.get("state") != "uploaded"
        ):
            raise ValueError(
                "Draft upload not confirmed; inspect and remove incomplete draft manually"
            )
        digest = uploaded.get("digest")
        if digest and digest != "sha256:" + package.sha(asset.read_bytes()):
            raise ValueError("Uploaded asset digest mismatch; do not publish draft")
    print("Created draft prerelease; human review/publication remains required.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--tag")
    parser.add_argument("--publish-draft", action="store_true")
    parser.add_argument("--audit-summary", type=Path)
    parser.add_argument("--repository")
    args = parser.parse_args()
    assets = assemble(args.directory, args.version, args.revision, args.tag)
    if args.publish_draft:
        if not args.tag or not args.repository or not args.audit_summary:
            raise ValueError("Draft creation requires an explicit tag/repository")
        audit = json.loads(args.audit_summary.read_text(encoding="utf-8"))
        if (
            audit.get("schema_version") != "cli_dependency_audit_v1"
            or audit.get("blocking") != []
            or audit.get("revision") != args.revision
            or audit.get("cargo_lock_sha256")
            != package.sha((package.ROOT / "Cargo.lock").read_bytes())
            or audit.get("targets") != sorted(package.TARGETS)
        ):
            raise ValueError("Missing, failed or mismatched CLI dependency audit")
        publish(args.repository, args.tag, args.revision, [*assets, args.audit_summary])
    else:
        print("Verified all four targets and generated SHA256SUMS/release-index.json")


if __name__ == "__main__":
    try:
        main()
    except (urllib.error.URLError, KeyError, ValueError, TypeError) as error:
        # Provider response bodies and credentials are never emitted.
        if isinstance(error, ValueError):
            raise SystemExit(str(error)) from None
        raise SystemExit(
            "Release API request/response failed; inspect the draft before retrying"
        ) from None
