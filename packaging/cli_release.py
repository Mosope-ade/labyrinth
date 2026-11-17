"""Build and verify CLI-only archives. Standard library only; no publication here."""

import argparse
import gzip
import hashlib
import io
import json
import os
import platform
import re
import stat
import struct
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

TARGETS = {
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64", "tar.gz"),
    "x86_64-pc-windows-msvc": ("Windows", "x86_64", "zip"),
    "x86_64-apple-darwin": ("Darwin", "x86_64", "tar.gz"),
    "aarch64-apple-darwin": ("Darwin", "aarch64", "tar.gz"),
}
ROOT = Path(__file__).resolve().parents[1]
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?\Z"
)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def identity(version, revision, target, tag=None):
    if (
        not SEMVER.fullmatch(version)
        or not re.fullmatch(r"[0-9a-f]{40}", revision)
        or target not in TARGETS
    ):
        raise ValueError("Invalid version, revision or target")
    if tag is not None and tag != f"cli-v{version}":
        raise ValueError("CLI tag must exactly match the Cargo version")
    return f"laby-v{version}-{target}"


def architecture(data, target):
    if "linux" in target:
        valid = (
            len(data) >= 20
            and data[:6] == b"\x7fELF\x02\x01"
            and struct.unpack_from("<H", data, 18)[0] == 62
        )
    elif "windows" in target:
        offset = struct.unpack_from("<I", data, 60)[0] if len(data) >= 64 else len(data)
        valid = (
            data[:2] == b"MZ"
            and offset <= 65536
            and len(data) >= offset + 6
            and data[offset : offset + 4] == b"PE\0\0"
            and struct.unpack_from("<H", data, offset + 4)[0] == 0x8664
        )
    else:
        cpu = 0x0100000C if target.startswith("aarch64") else 0x01000007
        valid = (
            len(data) >= 8
            and data[:4] == b"\xcf\xfa\xed\xfe"
            and struct.unpack_from("<I", data, 4)[0] == cpu
        )
    if not valid:
        raise ValueError("Executable format/architecture does not match target")


def native(target):
    system, machine, _ = TARGETS[target]
    actual = platform.machine().lower()
    actual = {"amd64": "x86_64", "arm64": "aarch64"}.get(actual, actual)
    if platform.system() != system or actual != machine:
        raise ValueError("Smoke test must run on the matching native platform")


def archive(payload, root, target):
    output = io.BytesIO()
    if TARGETS[target][2] == "zip":
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            for name, data in sorted(payload.items()):
                info = zipfile.ZipInfo(f"{root}/{name}", (1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = (
                    stat.S_IFREG | (0o755 if name in ("laby", "laby.exe") else 0o644)
                ) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                bundle.writestr(info, data)
    else:
        with (
            gzip.GzipFile(
                fileobj=output, mode="wb", mtime=0, filename=""
            ) as compressed,
            tarfile.open(
                fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT
            ) as bundle,
        ):
            for name, data in sorted(payload.items()):
                info = tarfile.TarInfo(f"{root}/{name}")
                info.size = len(data)
                info.mode = 0o755 if name == "laby" else 0o644
                bundle.addfile(info, io.BytesIO(data))
    return output.getvalue()


def dependencies(metadata):
    """Target-filtered Cargo graph; excludes dev-only nodes and implementation crates."""
    packages = {p["id"]: p for p in metadata["packages"]}
    roots = [p for p in packages.values() if p["name"] == "labyrinth-cli"]
    if len(roots) != 1:
        raise ValueError("Exactly one CLI package required")
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    pending, visited = [roots[0]["id"]], set()
    while pending:
        current = pending.pop()
        if current in visited:
            continue
        visited.add(current)
        pending.extend(
            d["pkg"]
            for d in nodes[current]["deps"]
            if any(k["kind"] != "dev" for k in d["dep_kinds"])
        )
    selected = sorted(
        (packages[i] for i in visited), key=lambda p: (p["name"], p["version"])
    )
    if any(
        p["name"]
        in (
            "labyrinth-engine",
            "labyrinth-api",
            "labyrinth-worker",
            "labyrinth-service-auth",
        )
        for p in selected
    ):
        raise ValueError("CLI must not package backend implementation dependencies")
    return roots[0], selected


def notices(selected):
    payload, entries = {}, []
    for package in selected:
        if package["name"] == "labyrinth-cli":
            continue
        if not package.get("license") and not package.get("license_file"):
            raise ValueError(f"Missing license metadata: {package['name']}")
        directory = Path(package["manifest_path"]).parent
        candidates = sorted(
            p
            for p in directory.iterdir()
            if p.is_file()
            and p.name.upper().startswith(
                ("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT")
            )
        )
        if package.get("license_file"):
            candidates.append(directory / package["license_file"])
        paths = []
        for index, path in enumerate(dict.fromkeys(candidates)):
            if path.is_symlink() or not path.resolve().is_relative_to(
                directory.resolve()
            ):
                raise ValueError("License path must stay within its dependency source")
            if path.stat().st_size > 512 * 1024:
                raise ValueError("Unexpectedly large license text")
            name = (
                f"licenses/{package['name']}-{package['version']}/{index}-{path.name}"
            )
            payload[name] = path.read_bytes()
            paths.append(name)
        entries.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package.get("license"),
                "source": package.get("source"),
                "repository": package.get("repository"),
                "license_files": paths,
            }
        )
        if package["name"] == "libdbus-sys":
            # Include the exact bundled C source and COPYING, not just Rust wrapper notices.
            source = directory / "vendor" / "dbus"
            members = {
                str(p.relative_to(source)): p.read_bytes()
                for p in sorted(source.rglob("*"))
                if p.is_file() and not p.is_symlink()
            }
            if "COPYING" not in members:
                raise ValueError("Vendored D-Bus source/license missing")
            payload["native-sources/dbus.tar.gz"] = archive(
                members, "dbus", "x86_64-unknown-linux-gnu"
            )
    payload["THIRD_PARTY_NOTICES.json"] = (
        json.dumps(entries, indent=2, sort_keys=True) + "\n"
    ).encode()
    return payload


def build(binary, metadata_path, output, revision, target, tag=None):
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    cli, selected = dependencies(metadata)
    version = cli["version"]
    stem = identity(version, revision, target, tag)
    if (
        "windows" in target
        and os.environ.get("RUSTFLAGS") != "-C target-feature=+crt-static"
    ):
        raise ValueError(
            "Windows packaging requires the static MSVC runtime build flag"
        )
    data = binary.read_bytes()
    if len(data) > 128 * 1024 * 1024:
        raise ValueError("CLI executable too large")
    architecture(data, target)
    compiler = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not compiler.startswith("rustc 1.95.0 "):
        raise ValueError("Packaging requires pinned Rust 1.95.0")
    payload = notices(selected)
    sysroot = Path(
        subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    )
    rust_docs = sysroot / "share/doc/rust"
    copyright = rust_docs / "COPYRIGHT-library.html"
    if not copyright.is_file():
        raise ValueError("Rust standard-library copyright notices are required")
    payload["licenses/rust/COPYRIGHT-library.html"] = copyright.read_bytes()
    for license_file in sorted((rust_docs / "licenses").glob("*")):
        if license_file.is_file():
            payload[f"licenses/rust/{license_file.name}"] = license_file.read_bytes()
    payload["laby.exe" if "windows" in target else "laby"] = data
    payload["README.md"] = (
        (ROOT / "packaging/cli/INSTALL.md")
        .read_text(encoding="utf-8")
        .replace("/blob/main/", f"/blob/{revision}/")
        .encode()
    )
    license_path = ROOT / "LICENSE"
    if license_path.is_file():
        payload["LICENSE"] = license_path.read_bytes()
    elif tag:
        raise ValueError("Choose a project license before preparing a release tag")
    payload["BUILD.json"] = (
        json.dumps(
            {
                "schema_version": "cli_package_v1",
                "version": version,
                "revision": revision,
                "target": target,
                "tag": tag,
                "toolchain": "1.95.0",
                "executable_sha256": sha(data),
                "cargo_lock_sha256": sha((ROOT / "Cargo.lock").read_bytes()),
                "project_license_included": license_path.is_file(),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    ).encode()
    payload["FILES.sha256"] = "".join(
        f"{sha(data)}  {name}\n" for name, data in sorted(payload.items())
    ).encode()
    output.mkdir(parents=True, exist_ok=True)
    destination = output / f"{stem}.{TARGETS[target][2]}"
    # Never overwrite a previously built asset.
    with destination.open("xb") as file:
        file.write(archive(payload, stem, target))
    with destination.with_name(destination.name + ".sha256").open(
        "x", encoding="ascii", newline="\n"
    ) as file:
        file.write(f"{sha(destination.read_bytes())}  {destination.name}\n")
    return destination


def unpack(path, destination):
    """Validate everything before extraction; never trust archive member paths/types."""
    if destination.exists():
        raise ValueError("Extraction destination must be new")
    content = {}
    total = 0
    if path.stat().st_size > 128 * 1024 * 1024:
        raise ValueError("Archive exceeds compressed size bound")
    if path.name.endswith(".zip"):
        with zipfile.ZipFile(path) as bundle:
            for info in bundle.infolist():
                mode = info.external_attr >> 16
                total += info.file_size
                if total > 256 * 1024 * 1024 or len(content) >= 4096:
                    raise ValueError("Archive exceeds extraction bounds")
                if info.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG):
                    raise ValueError("Only regular files are supported")
                if info.filename in content:
                    raise ValueError("Duplicate archive path")
                content[info.filename] = (bundle.read(info), mode & 0o777)
    else:
        with tarfile.open(path, "r:gz") as bundle:
            for info in bundle:
                total += info.size
                if total > 256 * 1024 * 1024 or len(content) >= 4096:
                    raise ValueError("Archive exceeds extraction bounds")
                if not info.isfile() or info.name in content:
                    raise ValueError("Only unique regular files are supported")
                content[info.name] = (bundle.extractfile(info).read(), info.mode)
    roots = set()
    for name in content:
        parts = PurePosixPath(name).parts
        if (
            "\\" in name
            or not parts
            or name.startswith("/")
            or any(p in ("..", ".", "") or ":" in p for p in parts)
            or len(parts) < 2
        ):
            raise ValueError("Unsafe archive path")
        roots.add(parts[0])
    if len(roots) != 1:
        raise ValueError("Exactly one archive root required")
    destination.mkdir()
    for name, (data, mode) in content.items():
        file = destination.joinpath(*PurePosixPath(name).parts)
        file.parent.mkdir(parents=True, exist_ok=True)
        with file.open("xb") as output:
            output.write(data)
        file.chmod(mode)
    return destination / roots.pop()


def verify_tree(root, target, version, revision):
    build_info = json.loads((root / "BUILD.json").read_text(encoding="utf-8"))
    if build_info.get("schema_version") != "cli_package_v1" or root.name != identity(
        version, revision, target, build_info.get("tag")
    ):
        raise ValueError("Package root/schema mismatch")
    if (build_info["version"], build_info["target"], build_info["revision"]) != (
        version,
        target,
        revision,
    ):
        raise ValueError("Build provenance mismatch")
    expected_files = {
        str(p.relative_to(root)).replace(os.sep, "/")
        for p in root.rglob("*")
        if p.is_file()
    } - {"FILES.sha256"}
    found = set()
    for line in (root / "FILES.sha256").read_text(encoding="ascii").splitlines():
        checksum, name = line.split("  ", 1)
        relative = PurePosixPath(name)
        if (
            relative.is_absolute()
            or ".." in relative.parts
            or "\\" in name
            or ":" in name
        ):
            raise ValueError("Unsafe checksum inventory")
        if name in found or name not in expected_files:
            raise ValueError("Package inventory mismatch")
        found.add(name)
        if sha(root.joinpath(*relative.parts).read_bytes()) != checksum:
            raise ValueError("Package file checksum mismatch")
    if found != expected_files:
        raise ValueError("Incomplete package inventory")
    executable = root / ("laby.exe" if "windows" in target else "laby")
    data = executable.read_bytes()
    architecture(data, target)
    if sha(data) != build_info["executable_sha256"]:
        raise ValueError("Executable checksum mismatch")
    return build_info, executable


def smoke(path, target, version, revision):
    native(target)
    expected = path.with_name(path.name + ".sha256").read_text(encoding="ascii")
    if expected != f"{sha(path.read_bytes())}  {path.name}\n":
        raise ValueError("Archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="laby package smoke ") as directory:
        root = unpack(path, Path(directory) / "extracted")
        _, executable = verify_tree(root, target, version, revision)
        env = {
            k: v
            for k, v in os.environ.items()
            if not k.startswith(("LABY_", "LABYRINTH_"))
        }
        for args, success, marker in [
            (("--version",), True, f"laby {version}"),
            (("--help",), True, "download"),
            (("login", "--help"), True, "--callback-port"),
            (
                (
                    "--api-url",
                    "http://example.com",
                    "--supabase-url",
                    "https://example.supabase.co",
                    "--publishable-key",
                    "DO_NOT_PRINT_ME",
                    "whoami",
                ),
                False,
                "Invalid client configuration",
            ),
        ]:
            result = subprocess.run(
                [str(executable), *args],
                env=env,
                check=False,
                capture_output=True,
                text=True,
                timeout=20,
            )
            output = result.stdout + result.stderr
            if (
                (result.returncode == 0) != success
                or marker not in output
                or "DO_NOT_PRINT_ME" in output
            ):
                raise ValueError("Packaged executable smoke failed")
    print(f"Verified native CLI package: {target}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    package = sub.add_parser("build")
    for name in ("binary", "metadata", "output"):
        package.add_argument(f"--{name}", type=Path, required=True)
    package.add_argument("--revision", required=True)
    package.add_argument("--target", choices=TARGETS, required=True)
    package.add_argument("--tag")
    check = sub.add_parser("smoke")
    check.add_argument("--archive", type=Path, required=True)
    check.add_argument("--target", choices=TARGETS, required=True)
    check.add_argument("--version", required=True)
    check.add_argument("--revision", required=True)
    args = parser.parse_args()
    if args.command == "build":
        print(
            build(
                args.binary,
                args.metadata,
                args.output,
                args.revision,
                args.target,
                args.tag,
            )
        )
    else:
        smoke(args.archive, args.target, args.version, args.revision)


if __name__ == "__main__":
    main()
