## Resumo das alterações

- Descreva o objetivo técnico ou de negócio deste Pull Request.

## Verificação mínima antes do merge

- [ ] O nome da branch segue [docs/agents/BRANCH_POLICY.md](../docs/agents/BRANCH_POLICY.md) (`<tipo>/<slug-kebab-case>`)
- [ ] `cargo fmt --check` passou localmente
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passou com zero avisos
- [ ] `cargo test --locked` passou (incluindo contratos de SEO e isolamento multi-tenant)
- [ ] Paridade de chaves `i18n/{pt-BR,en,es}.json` e CSP estrita verificadas
- [ ] Documentação relevante em `docs/` e `docs/agents/PROJECT_STATE.md` atualizada
