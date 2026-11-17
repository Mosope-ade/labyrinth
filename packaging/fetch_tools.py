"""Fetch reviewed Linux x86-64 validation tools by exact upstream archive SHA-256."""

import argparse
import hashlib
import io
import json
import tarfile
import urllib.request
from pathlib import Path, PurePosixPath


def fetch(directory):
    lock = json.loads(
        Path(__file__).with_name("tools.lock.json").read_text(encoding="utf-8")
    )
    directory.mkdir(parents=True, exist_ok=True)
    for name, info in lock.items():
        if name not in ("ruff", "cargo-audit") or not info["url"].startswith(
            "https://github.com/"
        ):
            raise ValueError("Unreviewed tool source")
        request = urllib.request.Request(
            info["url"], headers={"User-Agent": "labyrinth-cli-validation"}
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(32 * 1024 * 1024 + 1)
        if (
            len(data) > 32 * 1024 * 1024
            or hashlib.sha256(data).hexdigest() != info["sha256"]
        ):
            raise ValueError("Validation tool checksum mismatch")
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as bundle:
            matches = [
                m for m in bundle if PurePosixPath(m.name).name == name and m.isfile()
            ]
            if len(matches) != 1 or matches[0].size > 128 * 1024 * 1024:
                raise ValueError("Unexpected tool archive inventory")
            executable = bundle.extractfile(matches[0]).read()
        destination = directory / name
        with destination.open("xb") as file:
            file.write(executable)
        destination.chmod(0o755)
        print(f"Verified {name} {info['version']}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    fetch(parser.parse_args().directory)
