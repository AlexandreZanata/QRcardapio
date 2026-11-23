# AI Agent Workflow

Every coding agent modifying QR Cardápio must follow this sequence:

1. **Read [AGENTS.md](../../AGENTS.md)** and [PROJECT_STATE.md](PROJECT_STATE.md).
2. **Locate the authoritative docs** for the affected subsystem (`docs/architecture/`, `docs/domain/`, `docs/security/`, `docs/design/`).
3. **Inspect existing code cleanly**—reuse `domain/`, `infrastructure/database/queries/`, `rendering/`, and `i18n/` helpers rather than duplicating logic.
4. **Respect modularity (No King Files)**: Add new queries in the entity-specific file under `src/infrastructure/database/queries/`, keep handlers thin, and keep source files focused (~50–350 lines).
5. **Keep i18n parity**: Update `i18n/pt-BR.json`, `i18n/en.json`, and `i18n/es.json` together with identical keys, and add `data-i18n*` hooks on UI elements.
6. **Verify security & isolation**: Check `TenantContext`, `AND establishment_id = ?`, `_csrf` validation, strict CSP compliance (no inline JS or `style=""`), and `resolve_safe_subpath`.
7. **Write/update tests**: Cover behavior, tenant isolation, and edge cases in `tests/`.
8. **Run mandatory verification**:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --locked
   ```
9. **Update affected documentation and [PROJECT_STATE.md](PROJECT_STATE.md)** before summarizing work.
10. **Commit and push to GitHub**: After all checks pass and docs are updated, always commit and push every completed change to GitHub (`git push origin main`).
