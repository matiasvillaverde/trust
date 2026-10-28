[Service deployment, MCP, authentication, and workflow skills](service.md) is the starting point for the current application.

# Trust Documentation

This directory contains the project documentation for Trust, a risk-managed trading system built as a Rust workspace.

## Start Here

- [Risk contract](risk-contract.md): capital preservation, adaptive sizing, hard ceilings and remaining account-wide loss-budget requirements.
- [Agent skills](skills.md): install the MCP/skill package, select workflows, load references and test agent behavior.

- [Getting Started](getting-started.md): local setup, database setup, broker keys, and first risk-managed trade.
- [Architecture](architecture.md): crate boundaries, data flow, trade state machine, broker integration, and sync model.
- [CLI Reference](cli-reference.md): command groups, common flags, and representative examples.
- [Core Concepts](core-concepts.md): risk rules, levels, advisors, grading, session plans, mistakes, and distribution.

## Specialized Notes

- [Interactive Brokers E2E](interactive-brokers-e2e.md)
- [Encryption, secret custody, migration and server hardening](security.md)
- [Self-hosted authentication and security keys](authentication.md)
- [IBKR paper evidence and independent reconciliation](ibkr-paper-evidence.md)
- [Actual agent behavior evaluations](reliability/agent-evaluations.md)
- [Multi-Asset Completion Audit](multi-asset-completion-audit.md)
- [Trust Proof](trust-proof.md)
- [Reliability: feature inventory](reliability/features.md)
- [Reliability: extensive edge-case register](reliability/edge-cases.md)
- [Reliability: test evidence and limits](reliability/verification.md)
