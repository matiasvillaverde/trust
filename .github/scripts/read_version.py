"""Read and validate a release version without interpreting manifest text as shell code."""

import re
import sys
import tomllib
from pathlib import Path

IDENTIFIER = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
VERSION = re.compile(
    r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    rf"(?:-{IDENTIFIER}(?:\.{IDENTIFIER})*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)


def parse_version(document: str) -> str:
    manifest = tomllib.loads(document)
    package = manifest.get("workspace", {}).get("package", manifest.get("package", {}))
    version = package.get("version")
    if not isinstance(version, str) or VERSION.fullmatch(version) is None:
        raise ValueError("Manifest must contain a valid semantic release version.")
    return version


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: read_version.py Cargo.toml|-", file=sys.stderr)
        return 2
    try:
        document = sys.stdin.read() if sys.argv[1] == "-" else Path(sys.argv[1]).read_text()
        print(parse_version(document))
    except (OSError, ValueError, TypeError, AttributeError):
        print("Cannot read a valid release version from the manifest.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
