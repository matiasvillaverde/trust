# Command boundaries and module size

The default CLI and MCP are clients of the authenticated service. The feature-gated legacy CLI remains for migration and regression coverage. Its dispatcher is now a composition root: it constructs `TrustFacade`, selects a typed command, and invokes it.

The GoF Command pattern gives each command group its own workflow module and a common `execute` interface. `TrustFacade` is the receiver and owns financial decisions. Formatted report and market-data subcommands use function-pointer strategies rather than one wrapper class per function. Shared parsing, account lookup, and presentation have separate modules. No command handler receives the dispatcher or reaches through another command handler to perform a financial mutation.

## Enforced size limits

- New Rust production files: at most **800 physical lines**; aim for 300–500.
- New Rust test files: at most **1,000 physical lines**, grouped by business behavior.
- Existing larger files have explicit, fixed allowances in `scripts/architecture_budget.json`. An allowance may shrink or disappear; it may not grow or be added to a new file after this initial inventory.
- Generated Diesel schema is exempt. Handwritten code, comments, and blank lines all count.
- `make architecture-check` checks sizes and its own behavior tests. CI compares allowances with the base revision, preventing a change from hiding growth by increasing the budget.

A passing size gate is necessary, not sufficient. Split by responsibility, shared invariants, and reasons to change. Do not create numbered production shards, `include!` fragments, blanket lint exemptions, or a giant shared context that merely relocates a monolith. Prefer a small function or module over a new trait when there is only one implementation. Introduce GoF strategies, commands, or adapters only for a real boundary or variation.

Keep constructors and routing free of business policy. Keep input parsing out of calculations and persistence adapters. A cross-layer change must identify its owning layer and exercise observable results, rejected operations, persistence, and rollback at the lowest useful boundary. Tests should assert contracts, not private function arrangements or printed warnings.

The current allowances expose remaining debt in other large files; this change does not claim that every crate is already below the new-file limit. When touching one, extract the relevant responsibility and lower its allowance rather than extending it.
