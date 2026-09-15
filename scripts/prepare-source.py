#!/usr/bin/env python3
"""Keep Xcode's Cargo build locked to the source revision's dependencies."""
from pathlib import Path
import sys

source = Path(sys.argv[1])
script = source / "ios/script/cargo-build-ios"
original = script.read_text()
needle = "cargo build \\\n"
if original.count(needle) != 1:
    raise SystemExit("Upstream Cargo build entry changed; review before building.")
script.write_text(original.replace(needle, "cargo build --locked \\\n"))
