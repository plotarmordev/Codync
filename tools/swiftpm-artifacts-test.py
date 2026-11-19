"""Self-check for swiftpm-artifacts.py: python3 tools/swiftpm-artifacts-test.py"""

import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("swiftpm_artifacts", pathlib.Path(__file__).with_name("swiftpm-artifacts.py"))
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)

url = "https://github.com/stasel/WebRTC/releases/download/153.0.0/WebRTC-M153.xcframework.zip"
checksum = "3e3a8946f27510133e3feed04d05fa23505bbe366e977620503bfc7986c2b78f"

# The name SwiftPM looks up in its cache; any other name and it downloads (and traps) again.
assert artifacts.cache_key(url) == "https___github_com_stasel_WebRTC_releases_download_153_0_0_WebRTC_M153_xcframework_zip"

# Remote binary targets only: package dependencies and local binary targets have nothing to download.
manifest = f"""
    dependencies: [.package(url: "https://github.com/example/Archive.zip", exact: "1.0.0")],
    targets: [
        .binaryTarget(
            name: "WebRTC",
            url: "{url}",
            checksum: "{checksum}"
        ),
        .binaryTarget(name: "Local", path: "Local.xcframework.zip"),
    ]
"""
assert artifacts.binary_artifacts(manifest) == [(url, checksum)]

print("ok")
