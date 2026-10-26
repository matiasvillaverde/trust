# Trust service

The service owns authenticated operations, account authorization, durable operation records,
and the single financial worker. HTTP, MCP, and CLI are presentation clients of this boundary.

- Keep all financial mutations behind `TrustFacade`; never call a broker from a transport.
- Authenticate each HTTP request. Derive identity from verified credentials, never tool arguments.
- Check scope and every affected account before reading data or replaying a stored result.
- Every mutating tool requires an idempotency key. Persist intent and audit before dispatch.
- An unfinished operation after a crash has an unknown outcome. Never automatically replay it.
- Money is represented by decimal strings. Never expose credential values or password hashes.
- Catalogue entries must have real handlers, strict input schemas and useful next actions.
- Service/control storage is separate from financial storage. Back up both while the worker is stopped.
- Test auth failures, cross-account requests, replay/restart behavior, and real HTTP/MCP flows.
