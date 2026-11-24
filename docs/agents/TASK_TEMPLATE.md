# Task Implementation & Verification Checklist

Copy and fill out this checklist when planning or completing significant changes:

- [ ] Read `AGENTS.md` and authoritative module documentation
- [ ] Confirmed change stays within MVP scope (or explicitly approved expansion)
- [ ] Kept files cohesive and modular (no monolithic "king files")
- [ ] Verified tenant isolation (`TenantContext` + `establishment_id` query filtering)
- [ ] Verified CSRF token validation on new/modified `POST` handlers
- [ ] Gated new protected operations through `authorize` (lifecycle, suspension, role, enforcement, capability) and updated `contracts/` + module docs
- [ ] Kept money in integer centavos sourced from the commercial catalog (no hard-coded prices)
- [ ] Verified strict CSP compliance (zero inline `<script>` or `style="..."`)
- [ ] Updated `i18n/pt-BR.json`, `i18n/en.json`, `i18n/es.json` with identical keys + `data-i18n` attributes
- [ ] Added/updated automated unit and integration tests
- [ ] Ran `cargo fmt --check`
- [ ] Ran `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] Ran `cargo test --locked`
- [ ] Measured performance impact (if rendering, publishing, or DB paths changed)
- [ ] Updated affected `docs/` and `docs/agents/PROJECT_STATE.md`
