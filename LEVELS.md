# Level System Playbook

## What the feature does
- Each account has a trading `level` from `0..=4`.
- Level controls `risk_multiplier`:
- `0 -> 0.10x`, `1 -> 0.25x`, `2 -> 0.50x`, `3 -> 1.00x`, `4 -> 1.50x`.
- Every transition produces an immutable `level_change` audit event.

## Transition behavior
- Upgrades set status to `normal`.
- Downgrades set status to `probation`.
- Same-level transitions are rejected.
- Levels outside `0..=4` are rejected.

## Automatic policy
- Risk-first downgrade when:
- `monthly_loss_percentage <= -5` OR `largest_loss_percentage <= -2`.
- Exceptional-performance cooldown when:
- `profitable_trades >= 20`, `win_rate_percentage >= 85`, `consecutive_wins >= 8`.
- This triggers a temporary one-level downgrade with `cooldown` status.
- Cooldown recovery when:
- `profitable_trades >= 5`, `win_rate_percentage >= 65`, `consecutive_wins >= 2`.
- This upgrades one level and returns to `normal`.
- Upgrade when all pass:
- `profitable_trades >= 10`, `win_rate_percentage >= 70`, `consecutive_wins >= 3`.
- Automatic policy moves one step up/down only.

## Manual operations
- Manual change path is atomic (savepoint).
- Manual retries with the same target/reason/trigger in a short window are idempotent.
- Protected mutations require `--confirm-protected <KEYWORD>`.
- Configure keyword in keychain:
- `trust keys protected-set --value "<KEYWORD>"`
- Verify configuration:
- `trust keys protected-show`
- Remove configuration:
- `trust keys protected-delete`
- Use `trust level change --to <0-4> --reason "<text>" --trigger <trigger> --confirm-protected "<KEYWORD>"`.

## Trigger taxonomy
- Built-in triggers:
- `manual_override`
- `manual_review`
- `monthly_loss`
- `large_loss`
- `risk_breach`
- `performance_upgrade`
- `consecutive_wins`
- `account_creation`
- Custom triggers are accepted and normalized to lowercase.
- Use `trust level triggers` to list supported values.

## CLI UX contract
- `trust level status` shows current profile.
- `trust level history` shows audit trail.
- `trust level evaluate` supports dry-run and `--apply` (protected when applying).
- `--format json` returns stable machine-oriented payloads for agents.
- `trust onboarding status --format json` exposes setup readiness.
- `trust policy --format json` exposes protected vs unrestricted operation boundaries.
