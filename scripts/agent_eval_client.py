#!/usr/bin/env python3
"""Local-only MCP bridge for source-blind agent evaluations. Never prints credentials."""
import argparse
import json
from pathlib import Path
import sys
import urllib.parse
import urllib.request
import uuid


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise RuntimeError("Redirect refused")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("command", choices=["discover", "describe", "resource", "call"])
    parser.add_argument("name", nargs="?")
    parser.add_argument("arguments", nargs="?", default="{}")
    parser.add_argument("--inputs-only", action="store_true", help="For describe, omit the potentially large output schema")
    args = parser.parse_args()
    config = json.loads(args.session.read_text())
    url = urllib.parse.urlsplit(config["url"])
    if url.scheme != "http" or url.hostname not in {"localhost", "127.0.0.1"} or url.username or url.password or url.query or url.fragment:
        raise ValueError("Evaluation bridge only accepts a loopback service")
    opener = urllib.request.build_opener(NoRedirect())

    def rpc(method, params):
        request = urllib.request.Request(config["url"] + "/mcp", method="POST",
            headers={"Authorization": "Bearer " + config["token"], "Host": "localhost:8000",
                     "Content-Type": "application/json", "Accept": "application/json, text/event-stream",
                     "MCP-Protocol-Version": "2025-11-25"},
            data=json.dumps({"jsonrpc": "2.0", "id": str(uuid.uuid4()), "method": method, "params": params}).encode())
        with opener.open(request, timeout=50) as response:
            document = json.load(response)
        with args.session.with_name("calls.jsonl").open("a") as trace:
            trace.write(json.dumps({"method": method, "params": params, "result": document}) + "\n")
        if "error" in document:
            return document
        return document["result"]

    if args.command in {"discover", "describe"}:
        catalogue = rpc("tools/list", {})["tools"]
        if args.command == "describe":
            result = next((tool for tool in catalogue if tool["name"] == args.name), {"error": "Tool not available"})
            if args.inputs_only:
                result = {key: value for key, value in result.items() if key != "outputSchema"}
        else:
            info = rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "agent-evaluation", "version": "1"}})
            result = {"instructions": info.get("instructions"), "tools": [{"name": tool["name"], "description": tool.get("description")} for tool in catalogue], "resources": rpc("resources/list", {}).get("resources", [])}
    elif args.command == "resource":
        uri = args.name if args.name.startswith(("skill://trust/", "trust://skills/")) else "trust://skills/" + args.name
        result = rpc("resources/read", {"uri": uri})
    else:
        result = rpc("tools/call", {"name": args.name, "arguments": json.loads(args.arguments)})
        result = result.get("structuredContent", result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print(json.dumps({"error": "No confirmed bridge response. Inspect the original operation; do not repeat an uncertain intent."}))
        sys.exit(1)
