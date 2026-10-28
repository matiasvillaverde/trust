# Using Trust with an agent

Trust ships three workflow skills alongside its MCP service. Skills explain how to choose tools, interpret financial state, recover safely and report evidence. The service remains the authority for permissions, risk policy, idempotency, accounting and broker admission. Loading a skill never grants permission.

| Request | Skill | Expected result |
| --- | --- | --- |
| “Fetch quotes/bars or analyze portfolio exposure; no trade.” | `trust-risk` | Requested read-only research, source times and explicit data limitations. |
| “Size this proposal; don't trade.” | `trust-risk` | Read-only sizing, constraints and evidence; no draft or reservation. |
| “Execute this entry within my mandate.” | `trust-risk` | Exact bracket, staged execution and evidenced final state. |
| “Did my timed-out submission happen?” | `trust-operations` | Original operation lookup, reconciliation or a precise owner handoff; no duplicate entry. |
| “Protect gains / inspect this partial fill / exit.” | `trust-operations` | Lifecycle-aware protection or exit, with requested versus confirmed effects distinguished. |
| “Review my session and trades.” | `trust-operations` | Read-only review or explicitly authorized journal/grade records. |
| “Set limits / authorize an agent / recover a halt.” | `trust-admin` in the human session; `trust-risk` owner-handoff reference in an agent session | Owner workflow or a precise handoff, without elevating the agent. |

## Connect and install

First deploy Trust and configure OAuth/OIDC using [the service guide](service.md) and [authentication guide](authentication.md). Use a restricted agent subject for autonomous trading and a separate human-controlled owner session for administration. A package does not deploy Trust, register an OAuth client, configure broker credentials or grant scopes. Confirm the account/environment after connecting.

Build a portable Agent Plugins package locally, using your actual MCP endpoint:

```bash
python3 scripts/package_skills.py \
  --mcp-url https://trust.example.com/mcp \
  --output .local/trust-agent-plugin
```

The default package contains `trust-risk` and `trust-operations`. To build a separate package with owner guidance:

```bash
python3 scripts/package_skills.py \
  --mcp-url https://trust.example.com/mcp \
  --audience owner \
  --output .local/trust-owner-plugin
```

Choose a new output directory for each build; the command refuses to overwrite an existing package. It runs offline, rejects credential-bearing URLs and non-loopback HTTP, and copies only allowlisted skill files. It creates `plugin.json`, `mcp.json`, client display/dependency metadata, and `skills-manifest.json` with content hashes. The endpoint-specific dependency is added only to packaged `agents/openai.yaml`; Markdown stays byte-identical to the service's source. No tokens, broker keys, scripts that bypass the service, or secrets are bundled. Audience controls instructions, not account authority.

Install the output directory through a client supporting the [Agent Plugins format](https://agent-plugins.org/specification). OpenAI documents [portable packaging and client-specific OAuth setup](https://developers.openai.com/plugins/build/plugins). The portable MCP configuration contains only the transport and endpoint; OAuth is negotiated by the client using Trust's authentication challenge and discovery. Register the selected client's callback and OAuth client at your issuer as required. The package deliberately does not invent a deployment-specific client ID, embed client-specific authentication extensions, or enable dynamic registration.

For clients using filesystem Agent Skills, copy the complete desired directories from the package's `skills/` into the client's skill location, preserving `references/` and `agents/`. Codex supports repository/user `.agents/skills/`; see [OpenAI skill installation guidance](https://learn.chatgpt.com/docs/build-skills). Avoid overwriting a customized installation. Configure the authenticated MCP connection separately if the client does not consume the package's `mcp.json`. Install once, rather than duplicating the same skill through multiple discovery sources.

Client loading/OAuth behavior must be acceptance-tested in the specific application/version. The local automated checks establish package structure, file delivery and simulator behavior; they do not establish native activation in every host.

## Use without installing files

Trust exposes the same source through authenticated MCP:

1. Discover `skills_list` to choose by name and description. Its paginated data includes version, entrypoint, legacy URI and file metadata (path, URI, MIME type, SHA-256 and byte size).
2. Call `skill_read` with a name, or read the entrypoint resource. Load only the supporting reference relevant to the task.
3. Discover the actual tool schemas and follow the selected workflow under the caller's existing permissions.

```json
{"name":"trust-risk"}
```

Supporting reference through `skill_read`:

```json
{"name":"trust-risk","path":"references/trade-entry.md"}
```

Equivalent resource:

```text
skill://trust/trust-risk/references/trade-entry.md
```

The original `trust://skills/trust-risk` entrypoint remains supported, as do the other two legacy entrypoint URIs. Resource reads resolve exact bundled paths; filesystem paths, traversal, encoded aliases, arbitrary remote resources and other skills' files are not accepted. Admin metadata and resources are hidden from identities without admin permission. Installed owner instructions are not confidential credentials, but they do not elevate an agent.

Prompts use the same descriptions/content and accept optional `account_id` and `trade_id` UUIDs; a trade requires an account. Account authorization is checked before returning contextual guidance. Prompts do not execute tools. Resources/prompts do not guarantee automatic native skill activation: the client chooses how to load them.

The protocol [MCP Skills extension](https://skills.extensions.modelcontextprotocol.io/specification/stable/skills) defines `skills/list` and `skills/get`. Trust currently serves ordinary resources, prompts and the custom `skills_list`/`skill_read` tools; it does **not** advertise that extension. Its adapter and end-user client support require separate qualification. Do not confuse the underscore tool name with the slash protocol method. Resource hashes identify bytes but are not signatures or an authorization mechanism; distribute through a trusted release/source and authenticated TLS.

## Example user requests

After installation, hosts with explicit `$skill` invocation can use:

- `$trust-risk Assess a long TEST proposal at entry 100, stop 95, target 110 in my selected paper account. Size only; no mutations.`
- `$trust-operations Inspect the trade and original submission key I supplied. Explain the confirmed outcome and missing evidence.`
- `$trust-operations Review the strongest verified stop for this filled trade and tighten it only within my existing mandate.`
- `$trust-admin Explain the current account blockers and the exact owner changes needed. Do not change policy yet.`

Use real selected account/instrument/trade IDs through discovery; the numeric examples are illustrations, not trade recommendations. Skills are also eligible for normal automatic selection. The agent must preserve a read-only request and distinguish preparation, reservation, submission and confirmed execution. A known mandate authorizes its existing scope without repeated confirmations; missing account selection or financial-policy values still require clarification.

## Maintenance and evaluation

The three `skills/*/SKILL.md` files are the canonical entrypoints. Detailed procedures and worked calls live in their `references/`; schemas remain authoritative in MCP discovery. `service/src/skills.rs` embeds the reviewed files, so no request-controlled filesystem read or network fetch can supply instructions. Descriptions are derived from frontmatter, and hashes are cached once per service process.

On first connection or a service change, compare the installed package's version and selected Markdown hashes with `skills_list`. Compare Markdown only: packaged `agents/openai.yaml` intentionally contains deployment-specific metadata. A version match alone does not establish content parity. If parity is unknown or different, read the service's entrypoint and needed reference through MCP for that session. Updating an installed package is a separate operator action. Reuse verified tool discovery within the same connection; account state, permissions and financial capacity still require current evidence.

```bash
make skills-check
```

This checks examples against strict tool schemas, links and complete embedded-file parity, metadata/digests, every tool's workflow routing, owner visibility, path rejection, authenticated MCP delivery and package security. The normal service test suite and Python CI checks include these tests.

For behavior, use the [source-blind agent evaluation harness](reliability/agent-evaluations.md). Test discovery, actual skill/reference loading, safe execution and stopping conditions with the offline IBKR simulator. Retain failed trials, reports, selected skill hashes, calls and broker-effect counts; improve instructions based on observed failures. Record report correctness with the independent review rubric: an incomplete review or failed behavior check holds the overall trial. Repeat across target models and client versions; a few passing trials are not a general reliability guarantee or real IBKR acceptance evidence.

The [skill research and product comparisons](skills-comparison.md) record the primary sources, patterns adopted, measured selection improvement, and remaining qualification work. The [selection evaluation procedure](reliability/agent-evaluations.md#evaluate-skill-selection-separately) covers relevant and irrelevant prompts independently of MCP execution. Keep evaluation integrity and financial safety ahead of prompt size or speed.

Authoring follows the [Agent Skills format](https://agentskills.io/specification), [OpenAI's MCP skill guidance](https://developers.openai.com/plugins/build/skills), and [Anthropic's evaluation-driven authoring guidance](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices): focused descriptions, small entrypoints, conditional references and realistic behavioral tests.
