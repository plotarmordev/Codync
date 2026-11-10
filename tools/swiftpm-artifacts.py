"""Puts the binary artifacts of apps/Codync.xcodeproj's pinned packages into SwiftPM's cache.

SwiftPM downloads binary targets (WebRTC's xcframework) with URLSession, which traps (exit 133)
in a process outside a macOS login session, such as an agent run by a LaunchDaemon. With the
archive already in its cache, SwiftPM copies it instead and still checks its checksum.

    python3 tools/swiftpm-artifacts.py
"""

import hashlib
import json
import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
RESOLVED = ROOT / "apps/Codync.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved"
CACHE = pathlib.Path.home() / "Library/Caches/org.swift.swiftpm/artifacts"
BINARY_TARGET = re.compile(r'url:\s*"(https://[^"]+\.zip)",\s*checksum:\s*"([0-9a-f]{64})"')


def cache_key(url):
    """SwiftPM's cache file name for an artifact URL (spm_mangledToC99ExtendedIdentifier on ASCII)."""
    return re.sub(r"[^A-Za-z0-9_]", "_", url)


def binary_artifacts(manifest):
    """The (url, checksum) of each remote binary target in a Package.swift."""
    return BINARY_TARGET.findall(manifest)


def fetch(url, destination):
    subprocess.run(["curl", "--fail", "--silent", "--show-error", "--location", "--output", destination, url],
                   check=True)


def manifest(location, revision):
    repository = re.fullmatch(r"https://github\.com/(.+?)(?:\.git)?", location)
    if not repository:
        return ""
    with tempfile.NamedTemporaryFile() as file:
        fetch(f"https://raw.githubusercontent.com/{repository[1]}/{revision}/Package.swift", file.name)
        return pathlib.Path(file.name).read_text()


def seed(url, checksum):
    target = CACHE / cache_key(url)
    if target.exists():
        return
    with tempfile.TemporaryDirectory(dir=CACHE) as directory:
        archive = pathlib.Path(directory) / "archive.zip"
        fetch(url, archive)
        actual = hashlib.sha256(archive.read_bytes()).hexdigest()
        if actual != checksum:
            raise SystemExit(f"{url}: checksum {actual}, expected {checksum}")
        archive.rename(target)
    print(f"cached {url}")


def main():
    CACHE.mkdir(parents=True, exist_ok=True)
    for pin in json.loads(RESOLVED.read_text())["pins"]:
        for url, checksum in binary_artifacts(manifest(pin["location"], pin["state"]["revision"])):
            seed(url, checksum)


if __name__ == "__main__":
    main()
