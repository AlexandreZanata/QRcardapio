# Política de Nomeação de Branches e Proteção da `main`

A branch `main` é protegida e recebe alterações **exclusivamente via Pull Request** após aprovação das verificações obrigatórias do GitHub Actions.

## Padrão Obrigatório de Nomeação

Todas as branches criadas para Pull Requests devem obedecer estritamente à expressão regular:

```regex
^(feat|fix|chore|docs|refactor|test|perf|ci|seo|hotfix)/[a-z0-9]+(-[a-z0-9]+)*$
```

Regras adicionais:
- Apenas letras minúsculas (`a-z`), dígitos (`0-9`) e hífen (`-`) após a barra `/`.
- Sem maiúsculas, espaços, underscores (`_`), acentos ou hífens consecutivos (`--`) ou no início/fim da descrição.
- Comprimento máximo total: **64 caracteres**.
- Validação automática em todo PR pelo job `branch-policy` em [.github/workflows/ci.yml](../../.github/workflows/ci.yml).

## Prefixos Permitidos

| Prefixo | Uso | Exemplo válido |
|---|---|---|
| `feat/` | Nova funcionalidade ou módulo do produto | `feat/seo-geo-growth` |
| `seo/` | Otimizações dedicadas de SEO, schema ou conteúdo orgânico | `seo/rich-snippets-menu` |
| `fix/` | Correção de bug em ambiente normal | `fix/menu-slug-redirect` |
| `hotfix/` | Correção urgente para incidente em produção | `hotfix/sitemap-etag-fix` |
| `docs/` | Documentação técnica, guias e políticas | `docs/branch-protection-guide` |
| `test/` | Adição ou ampliação de suítes de teste e contratos | `test/seo-contract-suite` |
| `refactor/` | Reestruturação interna sem mudança de comportamento | `refactor/split-seo-handlers` |
| `perf/` | Melhoria de latência, Core Web Vitals ou throughput | `perf/lcp-image-preload` |
| `ci/` | Pipelines do GitHub Actions, caches e validações | `ci/pr-required-checks` |
| `chore/` | Manutenção de dependências, scripts ou tarefas operacionais | `chore/update-lockfile` |

## Fluxo Obrigatório para Envio ao GitHub

1. Atualize sua base a partir da `origin/main` e crie a branch seguindo a política:
   ```bash
   git fetch origin
   git checkout -b feat/minha-alteracao origin/main
   ```
2. Execute localmente todas as verificações mínimas antes de enviar:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --locked
   ```
3. Envie a branch ao GitHub e abra o Pull Request para `main`:
   ```bash
   git push -u origin feat/minha-alteracao
   gh pr create --base main
   ```
4. Aguarde todos os checks obrigatórios do PR ficarem verdes (`branch-policy`, `seo-contracts`, `quality`) antes de concluir o merge na `main`. Nunca realize push direto nem `--force` na `main`.
