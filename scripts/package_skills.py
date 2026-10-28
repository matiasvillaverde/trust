#!/usr/bin/env python3
"""Build an offline Trust Agent Plugins package. Never connects or embeds credentials."""
import argparse
import hashlib
import ipaddress
import json
from pathlib import Path
import re
import shutil
import tempfile
import tomllib
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("trust-risk", "trust-operations", "trust-admin")
SCHEMA = "https://agent-plugins.org/schemas/1.0.0/"


def endpoint(value):
    """Accept a literal endpoint, never userinfo, token queries or placeholders."""
    if not value.isascii() or any(c.isspace() or ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError("Use an ASCII MCP endpoint without whitespace or credentials.")
    try:
        url = urllib.parse.urlsplit(value)
        host = url.hostname
        port = url.port
        valid = host and (port is None or 1 <= port <= 65535)
        valid = valid and not (url.username is not None or url.password is not None or "?" in value or "#" in value)
        valid = valid and url.path.endswith("/mcp") and bool(re.fullmatch(r"/[A-Za-z0-9/._~-]*mcp", url.path))
        valid = valid and all(p not in {".", ".."} for p in url.path.split("/"))
        valid = valid and "\\" not in value and "$" not in value and "%" not in value
        try:
            local = ipaddress.ip_address(host or "").is_loopback
        except ValueError:
            local = host == "localhost"
            valid = valid and bool(re.fullmatch(r"[A-Za-z0-9]+(?:[.-][A-Za-z0-9]+)*", host or ""))
        valid = valid and (url.scheme == "https" or (url.scheme == "http" and local))
        if not valid:
            raise ValueError
    except ValueError:
        raise ValueError("Use HTTPS (HTTP only for loopback), ending in /mcp, without credentials, queries or fragments.") from None
    return value


def metadata(directory):
    body = (directory / "SKILL.md").read_text()
    match = re.match(r"\A---\nname: ([a-z0-9-]+)\ndescription: ([^\n]+)\n---\n", body)
    if not match or match[1] != directory.name or not 1 <= len(match[2]) <= 1024:
        raise ValueError("Skill metadata must match its directory and use a plain single-line description.")
    return {"name": match[1], "description": match[2]}


def source_files(root, name):
    directory = root / "skills" / name
    if directory.is_symlink():
        raise ValueError("Skill sources must not contain symlinks.")
    entries = sorted(directory.rglob("*"))
    for path in entries:
        if path.is_symlink() or (not path.is_file() and not path.is_dir()):
            raise ValueError("Skill sources must contain only regular files and directories.")
        if path.is_file() and not (
            path.name == "SKILL.md" and path.parent == directory
            or path.parent == directory / "references" and path.suffix == ".md"
            or path == directory / "agents" / "openai.yaml"
        ):
            raise ValueError("Unexpected file in skill source; refusing to package possible private data.")
    metadata(directory)
    return [p for p in entries if p.is_file()]


def build(output, mcp_url, audience="agent", root=ROOT):
    mcp_url = endpoint(mcp_url)
    if audience not in {"agent", "owner"}:
        raise ValueError("Audience must be agent or owner.")
    if output.exists() or output.is_symlink():
        raise ValueError("Output already exists; choose a new directory to preserve the installed package.")
    names = NAMES if audience == "owner" else NAMES[:2]
    sources = {name: source_files(root, name) for name in names}
    version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    output.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".trust-skills-", dir=output.parent))
    try:
        plugin = {"$schema": SCHEMA + "plugin.schema.json", "name": "trust-" + audience,
                  "version": version, "description": "Trust risk-managed trading workflows and authenticated MCP connection."}
        mcp = {"$schema": SCHEMA + "mcp.schema.json", "mcpServers": {"trust": {
            "type": "streamable-http", "url": mcp_url}}}
        for filename, document in (("plugin.json", plugin), ("mcp.json", mcp)):
            (stage / filename).write_text(json.dumps(document, indent=2) + "\n")
        skills = []
        for name, paths in sources.items():
            entry = metadata(root / "skills" / name)
            entry.update(version=version, files=[])
            for source in paths:
                relative = source.relative_to(root / "skills" / name)
                target = stage / "skills" / name / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(source.read_bytes())
                # Endpoint-specific client metadata is generated; canonical skill text stays identical.
                if relative.as_posix() == "agents/openai.yaml":
                    with target.open("a") as stream:
                        stream.write('\ndependencies:\n  tools:\n    - type: "mcp"\n      value: "trust"\n      description: "Authenticated Trust service"\n      transport: "streamable_http"\n      url: ' + json.dumps(mcp_url) + '\n')
                content = target.read_bytes()
                entry["files"].append({"path": relative.as_posix(), "size_bytes": len(content),
                                       "sha256": hashlib.sha256(content).hexdigest()})
            skills.append(entry)
        # Hashes identify content, not a signature or a grant of trust/permission.
        (stage / "skills-manifest.json").write_text(json.dumps({"version": version, "skills": skills}, indent=2) + "\n")
        # Reserve the destination without overwriting a concurrent build or symlink.
        output.mkdir()
        try:
            for child in stage.iterdir():
                shutil.move(str(child), output / child.name)
        except Exception:
            shutil.rmtree(output)
            raise
    finally:
        shutil.rmtree(stage)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mcp-url", required=True, help="Literal public MCP URL; OAuth credentials remain in the client")
    parser.add_argument("--output", required=True, type=Path, help="New output directory; existing paths are never overwritten")
    parser.add_argument("--audience", choices=["agent", "owner"], default="agent", help="Controls bundled skills only, never server permissions")
    args = parser.parse_args()
    try:
        result = build(args.output, args.mcp_url, args.audience)
    except (ValueError, OSError, KeyError) as error:
        # Do not echo supplied URLs or filesystem exceptions that may contain private values.
        parser.exit(2, "Package not built: " + (str(error) if isinstance(error, ValueError) else "check source and output paths") + "\n")
    print("Built Trust skill package at " + str(result))


if __name__ == "__main__":
    main()
