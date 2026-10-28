"""Second independent official MCP SDK; credentials exist only in a private test directory."""
import asyncio
import json
from pathlib import Path
import sys
import httpx2
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client


async def main():
    options = json.loads(Path(sys.argv[1]).read_text())
    async with httpx2.AsyncClient(headers={"Authorization": "Bearer " + options["token"]}, follow_redirects=False) as http:
        async with streamable_http_client(options["url"], http_client=http) as (reader, writer):
            async with ClientSession(reader, writer) as session:
                await session.initialize()
                catalogue = await session.list_tools()
                assert any(tool.name == "account_readiness" for tool in catalogue.tools)
                assert all(tool.name != "account_safety_policy_set" for tool in catalogue.tools)
                result = await session.call_tool("service_status", {})
                assert not result.is_error
                assert result.structured_content["data"]["role"] == "agent"
                resources = await session.list_resources()
                assert any(str(resource.uri) == "trust://skills/trust-risk" for resource in resources.resources)
    print("PASS official Python MCP SDK authenticates, discovers tools and reads agent state")


if __name__ == "__main__":
    asyncio.run(main())
