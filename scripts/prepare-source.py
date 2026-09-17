#!/usr/bin/env python3
"""Apply reviewed iOS fixes and lock Xcode's Cargo build dependencies."""
from pathlib import Path
import sys
import shutil
import subprocess

source = Path(sys.argv[1]).resolve()
build_root = Path(__file__).resolve().parent.parent
script = source / "ios/script/cargo-build-ios"
original = script.read_text()
needle = "cargo build \\\n"
if original.count(needle) != 1:
    raise SystemExit("Upstream Cargo build entry changed; review before building.")

patches = [
    build_root / "patches/ios-workspace-trust.patch",
    build_root / "patches/ios-keyboard-interactive.patch",
]
lock = build_root / "Cargo.lock.ios"
if not lock.is_file():
    raise SystemExit("Reviewed Cargo.lock.ios is missing; review before building.")

# Check all patch contexts before modifying any build input. A repeated run or
# upstream drift must fail instead of silently building without the UI fix.
subprocess.run(
    ["git", "apply", "--check", "--whitespace=error", *map(str, patches)],
    cwd=source,
    check=True,
)
# Include newly added source files in the shipped `git diff --binary HEAD`.
# --intent-to-add leaves their content unstaged while making the files visible to Git.
subprocess.run(
    ["git", "apply", "--intent-to-add", *map(str, patches)], cwd=source, check=True
)
script.write_text(original.replace(needle, "cargo build --locked \\\n"))
shutil.copyfile(lock, source / "Cargo.lock")
