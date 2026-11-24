# ADR-010: Expand-Contract Migrations and Rollback-Tolerant Binaries

- **Status**: Accepted
- **Date**: 2026-11-24

## Context

The VPS deploy agent (`deploy/vps/qrcardapio-deploy-agent.py`) rolls back by switching the binary symlink, not by restoring the database. `sqlx::Migrator::run` rejects a database that contains an applied migration unknown to the binary (`VersionMissing`). Therefore any release that adds a migration and then fails its smoke checks leaves the previous binary unable to start. The commercial foundation is the first change that must migrate live billing data, so rollback safety matters.

## Decision

1. **Expand-contract.** Schema changes are split into an *expand* release (new tables, new nullable or defaulted columns, backfills; old code keeps working) and, only after every binary that reads the old shape is retired, a *contract* release (drops or renames). Legacy billing tables (`billing_checkouts`, `billing_payments`, `billing_coverage`) are kept and remain the gateway source of truth.
2. **Tolerant migrator.** `init_pool` runs migrations with `ignore_missing = true`, so a binary can start against a database migrated by a newer release. This is safe only because of rule 1.
3. **Idempotent expand migrations.** `20261124000009_commercial_foundation.sql` uses `CREATE ... IF NOT EXISTS` and `INSERT ... ON CONFLICT DO NOTHING` and avoids `ALTER TABLE`, so it can be re-applied after a manual `_sqlx_migrations` rollback.
4. **Release-specific recovery.** Binaries released before this ADR do not ignore missing migrations. Rolling back this specific release to such a binary requires the manual procedure in `docs/operations/RUNBOOK.md#rollback-of-the-commercial-foundation-release`.

## Consequences

- Future rollbacks of expand releases are a symlink switch.
- Contract (destructive) migrations need an explicit two-release plan and a verified backup.
- An older binary running on a newer schema ignores tables it does not know; it cannot create contracts, so the runbook procedure re-runs the idempotent backfill on roll-forward.
