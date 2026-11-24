# Programa de Afiliados

Arquitetura modular, regras financeiras e proteções de lógica de negócio do programa de afiliados do **QR Cardápio**.

## 1. Visão Geral

Qualquer conta autenticada no QR Cardápio — **mesmo sem estabelecimento cadastrado** — pode ativar seu perfil de afiliado em `/dashboard/affiliate` (quando a autoativação está habilitada) ou ser cadastrada manualmente pelo administrador em `/admin/affiliates`.

Cada afiliado recebe um link exclusivo (`https://qrcardapio.com.br/r/{codigo}` ou `/?ref={codigo}`), uma **cota mensal configurável individualmente pelo administrador** (valor fixo em `R$/mês` ou percentual `%` da assinatura) e acompanha cliques, indicações, extrato de comissões e histórico de repasses via PIX.

## 2. Estrutura Modular (Anti-King-File)

| Camada | Módulos | Responsabilidade |
|---|---|---|
| Domain | `src/domain/affiliate.rs`, `src/domain/affiliate_math.rs` | Enums, validação de códigos/chaves PIX, aritmética 100% inteira (centavos e basis points), parcelamento mensal, canonicalização anti-autoindicação e mascaramento LGPD de e-mail. |
| Infrastructure | `src/infrastructure/database/queries/{affiliates,affiliate_referrals,affiliate_ledger,affiliate_payouts}.rs` | Persistência SQLite, sincronização idempotente dentro da transação de cobrança Asaas e baixa atômica de repasses via `BEGIN IMMEDIATE`. |
| Application | `src/application/{affiliates,affiliate_operations}.rs`, hooks em `application/{billing,establishments}.rs` | Regras globais, autoativação, cadastro/edição admin, captura de cliques, atribuição imutável no cadastro, desqualificação e liquidação de PIX. |
| HTTP & Rendering | `src/http/handlers/{affiliates,admin_affiliates,admin_affiliate_detail}.rs`, `src/rendering/affiliates.rs` | Captura `/r/{code}` e `/?ref=`, portal `/dashboard/affiliate`, painel `/admin/affiliates` e detalhe `/admin/affiliates/{id}`. |
| Templates & CSS | `templates/dashboard/{affiliate,admin_affiliates,admin_affiliate_detail}.html`, `static/css/affiliates.css` | SSR Askama com CSP estrita (zero inline JS/CSS) e 100% de paridade i18n em `pt-BR`, `en` e `es`. |

## 3. Regras de Negócio e Proteções Contra Falhas Financeiras

1. **Aritmética inteira estrita (zero floats)**:
   - Cotas fixas armazenadas em centavos (`0..=MONTHLY_PRICE_CENTAVOS`) e percentuais em basis points (`0..=10000`, onde `2000 = 20%`).
   - Toda comissão é estritamente limitada ao valor líquido efetivamente pago (`commission_centavos <= confirmed_centavos`).
2. **Liberação mês a mês em ciclos multi-mês (`1`, `6`, `12` meses)**:
   - Plano mensal gera 1 parcela; semestral gera 6 parcelas mensais; anual gera 12 parcelas mensais, com distribuição exata de resto de centavos (soma das parcelas sempre igual ao total exato).
   - Cada parcela possui `available_on = add_months_floor(due_date, idx - 1) + hold_days` (padrão `15` dias contra arrependimento CDC/chargeback). A disponibilidade é avaliada deterministicamente por `available_on <= today` (fuso UTC-3 de cobrança), sem depender de cron.
3. **Snapshot imutável de cota**:
   - Alterar a cota mensal de um afiliado no painel admin afeta **apenas pagamentos futuros**. Lançamentos já gravados preservam `snapshot_commission_type` e `snapshot_commission_value`.
4. **Estorno, chargeback, exclusão e compensação (clawback)**:
   - Sincronizado **dentro da mesma transação `BEGIN IMMEDIATE`** do webhook Asaas (`application::billing::process_webhook`).
   - Parcelas ainda não pagas (`pending`/`available`) passam atomicamente para `voided`.
   - Parcelas já pagas em um repasse anterior (`paid`) geram um lançamento negativo idempotente (`entry_kind = 'clawback'`, `commission_centavos = -valor`) abatido automaticamente dos próximos repasses. Repasses com saldo líquido `<= 0` são bloqueados.
   - Disputa de chargeback vencida posteriormente (`PAYMENT_RECEIVED` mais recente) restaura parcelas anuladas de indicações ainda elegíveis.
5. **Webhooks fora de ordem, duplicados e liquidação D+30**:
   - `UNIQUE (environment, payment_id, installment_index, entry_kind)` impede duplicação entre `PAYMENT_CONFIRMED` e `PAYMENT_RECEIVED`.
   - `sync_subscription_commissions` roda tanto em `Change::Payment` quanto ao vincular checkout em `Change::Subscription` e em `reconcile_subscription`, cobrindo webhooks fora de ordem.
   - `affiliate_evaluated_payments` registra pagamentos já avaliados na primeira confirmação válida, impedindo que um `PAYMENT_RECEIVED` 30 dias depois conceda comissão retroativa a um pagamento originalmente inelegível (ex.: afiliado revogado no dia da compra).
   - Pagamentos com valor divergente de `billing_checkouts.price_centavos` não geram comissão (mesma regra de `billing_coverage`).
6. **Prevenção de autoindicação e sequestro de atribuição**:
   - Atribuição gravada uma única vez na criação da conta (`/register` ou primeiro cadastro via Google OAuth) com `UNIQUE (referred_account_id)`; contas já existentes nunca são reatribuídas.
   - Bloqueia autoindicação por `account_id`, por identidade canônica de e-mail (`+alias` e pontos/equivalência `gmail.com`/`googlemail.com`) e novamente no webhook verificando `establishment_members`.
   - Links com `?ref=` redirecionam com `302` e `Cache-Control: no-store` para a URL canônica limpa, protegendo SEO e evitando cache de `Set-Cookie`. `/r/` está bloqueado em `robots.txt` e rejeita redirecionamentos abertos.
7. **Ciclo de vida de status (`active`, `paused`, `revoked`)**:
   - `paused`: bloqueia novos cliques/indicações, mantendo renovações de indicados antigos.
   - `revoked`: bloqueia novas indicações e interrompe novas comissões em renovações, preservando o histórico para auditoria.
8. **Isolamento de ambiente, privacidade LGPD e segurança**:
   - Comissões e repasses são isolados por `environment` (`sandbox` vs `production`).
   - O portal do afiliado mascara e-mails dos indicados (`ma***@dominio.com`); o painel admin exibe o e-mail completo e permite desqualificar indicações fraudulentas.
   - Todas as rotas `/admin/affiliates*` exigem `AdminContext`, confirmação explícita de baixa PIX e verificação `_csrf` em tempo constante.
