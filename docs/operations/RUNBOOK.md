# Operations Runbook

## Local Development

```bash
cp .env.example .env
./scripts/dev.sh
```

System packages for local OCR & PDF menu pre-registration (pre-installed in `Dockerfile`):
```bash
sudo apt-get install -y tesseract-ocr tesseract-ocr-por tesseract-ocr-eng tesseract-ocr-spa poppler-utils
```

Useful CLI commands:
```bash
qrcardapio serve
qrcardapio db migrate
qrcardapio admin create --email ops@qrcardapio.com.br --password "<strong-password>"
qrcardapio admin reset-token --email owner@restaurant.com.br
qrcardapio seed
qrcardapio seed-public-demo
qrcardapio republish-all
```

## Production Deployment (Single 1-vCPU / 1-GB VPS)

1. Build or deploy on the target host with Docker Compose:
   ```bash
   DOMAIN=qrcardapio.com.br PUBLIC_BASE_URL=https://qrcardapio.com.br docker compose up -d --build
   ```
2. Bootstrap the initial operational admin account inside the container:
   ```bash
   docker compose exec app qrcardapio admin create --email admin@qrcardapio.com.br --password "<strong-password>"
   ```
3. Verify readiness:
   ```bash
   curl -fsS http://127.0.0.1/healthz
   curl -fsS http://127.0.0.1/readyz
   ```

## Online Backups (`scripts/backup.sh`)

Runs `qrcardapio db backup` (`VACUUM INTO` + read-only `PRAGMA integrity_check`), archives `DATA_DIR/media`, writes `SHA256SUMS`, and prunes snapshots older than `RETENTION_DAYS` (default 14 days). Safe while the application is running.

```bash
DATA_DIR=/var/lib/qrcardapio QRCARDAPIO_BIN=/usr/local/bin/qrcardapio ./scripts/backup.sh /var/backups/qrcardapio
```

Schedule via cron or systemd timer (recommended daily off-peak + off-site sync of the timestamped backup directory).

## Verified Restore (`scripts/restore.sh`)

**Always stop the `qrcardapio` service first** before replacing the SQLite file:

```bash
docker compose stop app
DATA_DIR=/var/lib/qrcardapio ./scripts/restore.sh /var/backups/qrcardapio/20261123T120000Z
docker compose start app
```

`restore.sh` validates `SHA256SUMS`, removes stale `-wal`/`-shm` sidecars, verifies `PRAGMA integrity_check`, restores `media/`, and executes `qrcardapio republish-all` to regenerate static public snapshots cleanly.

## Logging & Incident Recovery

- Production sets `LOG_FORMAT=json` (`stdout`). Docker's `json-file` log driver or `journald` should rotate logs (`max-size: 20m, max-file: 5`).
- If a snapshot file becomes corrupted or missing on disk without touching SQLite, run `qrcardapio republish-all` to rebuild all published static menus cleanly.
- Never copy an active `qrcardapio.sqlite` without `VACUUM INTO` or stopping the process; never mount `DATA_DIR/db` over NFS.

### Reprocessamento de eventos do outbox

1. Quando `/readyz` indicar `outbox_lag`, abra **Operação → Reprocessamento de eventos** com uma conta administradora. `/admin/outbox` fornece os contadores por consumidor e a idade do evento pendente mais antigo.
2. Localize a falha pelo consumidor, identificador do evento e chave do erro. Corrija a causa na aplicação ou no ambiente antes de solicitar o reprocessamento.
3. Use **Reprocessar** na linha do evento. O worker recebe um novo ciclo de tentativas para aquele consumidor e mantém os eventos posteriores do mesmo agregado em espera até o sucesso.
4. Consulte o diagnóstico novamente para confirmar a queda dos contadores de falhas e eventos em espera. Um `409` pede atualizar o diagnóstico antes de tentar novamente. Se o mesmo evento voltar a falhar, use os novos detalhes para investigar a causa. A tabela `event_consumer_replays` registra a operação aceita.

### Falha na expiração de um pedido

Se o log indicar que a expiração foi adiada, identifique o pedido pelo ID registrado e investigue a chave do erro. O pedido segue na fila com nova tentativa após 30 segundos. Os outros pedidos vencidos podem avançar. Após corrigir a causa, confirme que o pedido foi rejeitado e que `OrderRejected` entrou no outbox antes de verificar a compensação do pagamento.

## Habilitação do Asaas

1. Separe a implantação de homologação da produção, inclusive banco, domínio HTTPS e chave de API. Configure `PUBLIC_BASE_URL` com a origem pública HTTPS e `COOKIE_SECURE=true`.
2. Configure `ASAAS_ENV=sandbox`, `ASAAS_API_KEY` com a credencial do Sandbox e `ASAAS_WEBHOOK_TOKEN` com um segredo diferente de pelo menos 32 caracteres. O Compose injeta essas variáveis no serviço `app`. Se usar um arquivo de ambiente, coloque o valor da chave entre aspas simples para preservar o caractere `$`. Proteja o arquivo e mantenha-o fora do Git. Com as três variáveis vazias, o sistema opera sem cobrança.
3. Na conta correspondente do Asaas, cadastre `https://<domínio>/webhooks/asaas` com o valor de `ASAAS_WEBHOOK_TOKEN` como token de autenticação, API v3 e entrega sequencial. Habilite `CHECKOUT_CREATED`, `CHECKOUT_PAID`, `CHECKOUT_CANCELED`, `CHECKOUT_EXPIRED`, `SUBSCRIPTION_CREATED`, `SUBSCRIPTION_UPDATED`, `SUBSCRIPTION_INACTIVATED`, `SUBSCRIPTION_DELETED`, `PAYMENT_CONFIRMED`, `PAYMENT_RECEIVED`, `PAYMENT_REFUNDED`, `PAYMENT_CHARGEBACK_REQUESTED`, `PAYMENT_CHARGEBACK_DISPUTE`, `PAYMENT_AWAITING_CHARGEBACK_REVERSAL` e `PAYMENT_DELETED`.
4. No Sandbox, valide criação e expiração do checkout, retorno ao painel, primeira cobrança e renovação, reentrega de evento, estorno e cancelamento. Confirme que o período pago habilita a publicação e que a URL pública abre sem sessão. Ao mudar para `ASAAS_ENV=production`, use a chave e o webhook da conta de produção.

Se o retorno do checkout ocorrer antes da confirmação, o painel exibe o estado pendente. Se a cobrança confirmar e o vínculo da assinatura ainda faltar, use **Conciliar com o Asaas**. Se um checkout conhecido permanecer aberto após o vencimento, confira seu status no Asaas e reenvie o evento de cancelamento ou expiração antes de abrir outro. Se uma assinatura ou cobrança estiver ausente após rollback ou restauração, reenvie os webhooks correspondentes e confirme a ativação antes de novas contratações. Use a referência local e o ID do checkout para localizar a operação. Evite repetir cobranças ou editar o SQLite manualmente.

Desabilitar a integração local interrompe o recebimento de eventos e remove a exigência de cobrança no painel. Assinaturas vigentes devem ser encerradas no Asaas se a operação comercial for interrompida. Antes de um rollback ou troca de ambiente, preserve o banco e os eventos recebidos.

[Sandbox Asaas](https://docs.asaas.com/docs/sandbox) · [Configurar o webhook](https://docs.asaas.com/docs/receba-eventos-do-asaas-no-seu-endpoint-de-webhook)

## Integrar ChatGPT, Claude e Antigravity

1. Implante a versão que inclui a migração `20261123000005_mcp_oauth.sql`. Configure `MCP_ENABLED=true`, `PUBLIC_BASE_URL` com a origem HTTPS pública e `COOKIE_SECURE=true`. O Compose encaminha a flag ao serviço `app`. Garanta que `/mcp`, `/mcp/oauth/*` e `/.well-known/oauth-*` cheguem ao Axum por HTTPS e sejam acessíveis aos clientes externos. A configuração de Caddy do repositório encaminha essas rotas pelo proxy geral.
2. Entre no painel com uma conta proprietária que já tem estabelecimento. Abra **Configurações → Integrações de IA** e copie o endereço MCP. Para o domínio principal, use `https://qrcardapio.com.br/mcp`.
3. No ChatGPT, abra **Plugins → Adicionar servidor MCP personalizado**, cole o endereço e escolha **OAuth** com **CIMD**. Instale o plugin criado e autorize o vínculo no QR Cardápio. A disponibilidade de conectores e ações de escrita depende do cliente e das permissões do workspace. [Adicionar servidor MCP personalizado no ChatGPT](https://developers.openai.com/api/docs/guides/custom-mcp-server) · [OAuth no ChatGPT](https://developers.openai.com/plugins/build/auth)
4. No Claude, abra **Customize → Connectors → Add custom connector**, informe o endereço MCP e escolha a identidade publicada do Claude para o OAuth. Conecte e autorize o vínculo. O servidor precisa estar acessível à infraestrutura do Claude. [Conectores personalizados no Claude](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp) · [Autenticação do Claude](https://claude.com/docs/connectors/building/authentication)
5. No Google Antigravity, use **Customizations → Installed MCP Servers** ou **MCP Servers → Manage MCP Servers** na IDE, conforme a superfície. Adicione a configuração abaixo e selecione **Authenticate**. Use uma versão da IDE com suporte a CIMD, conforme a documentação oficial. [MCP no Antigravity](https://antigravity.google/docs/mcp)

```json
{
  "mcpServers": {
    "qr-cardapio": {
      "serverUrl": "https://qrcardapio.com.br/mcp"
    }
  }
}
```

Depois de autorizar, volte ao cliente pelo link na tela de confirmação. Na homologação, peça para consultar a conta (`get_account_settings`) e o rascunho (`get_menu_draft`), ajustar a descrição/cor do estabelecimento ou criar/reordenar/editar itens no rascunho e, quando desejar, publicar ao vivo (`publish_menu`) e validar `/c/{slug}`. Revogue a conexão nas **Integrações de IA** e verifique que a sessão do cliente deixa de acessar a conta.

Para diagnosticar descoberta, consulte `/.well-known/oauth-protected-resource/mcp` e `/.well-known/oauth-authorization-server` na origem pública. Uma chamada sem credencial a `/mcp` deve retornar `401` com `WWW-Authenticate`. Se um cliente mostrar erro no callback, confira se usa a identidade publicada e a opção de autenticação OAuth indicada acima. O OAuth vincula credenciais à origem configurada. Alterações de domínio exigem nova conexão.

## Habilitação do login com o Google

1. No [Google Auth Platform](https://console.cloud.google.com/auth/overview), configure **Branding** com o nome QR Cardápio, e-mail de suporte, domínio autorizado, página inicial e política de privacidade pública do serviço. Em **Audience**, use a opção externa para atender estabelecimentos de qualquer organização. Durante os testes, inclua as contas piloto em **Test users**.
2. Em **Clients**, crie um cliente **Web application**. Cadastre `https://qrcardapio.com.br/auth/google/callback` como URI de redirecionamento autorizada em produção. Ela deve corresponder exatamente a `PUBLIC_BASE_URL` mais `/auth/google/callback`. Use um cliente separado para desenvolvimento, com o retorno na origem local, como `http://localhost:3000/auth/google/callback`.
3. No `.env` do servidor, preencha `GOOGLE_CLIENT_ID` e `GOOGLE_CLIENT_SECRET`. Em produção use `PUBLIC_BASE_URL=https://qrcardapio.com.br` e `COOKIE_SECURE=true`. Para HTTP em loopback no desenvolvimento, use `COOKIE_SECURE=false`. Mantenha o segredo no ambiente do servidor.
4. Valide a configuração e recrie o aplicativo com a nova imagem. A migração é aplicada na inicialização. Com as duas credenciais vazias, as telas seguem com login por senha.

```bash
docker compose config --quiet
docker compose up -d --build app
```

Em homologação, crie uma conta Google nova, termine o cadastro do estabelecimento, saia e volte a entrar. Teste também o vínculo usando uma conta existente com senha. Verifique a navegação do plano pago se o Asaas estiver habilitado e repita o login na tela de consentimento de cada conector MCP. Para abrir ao público, conclua as etapas de publicação, verificação de marca e domínio solicitadas pelo console.

[Configuração OAuth para web](https://developers.google.com/identity/protocols/oauth2/web-server) · [Políticas de autenticação](https://developers.google.com/identity/verification/authentication-policy-compliance)

## Autodeploy seguro na VPS (pull com atestação Sigstore)

O autodeploy de produção usa arquitetura **pull** sem chaves SSH, IPs ou segredos de VPS armazenados no GitHub:

1. **Empacotamento e assinatura no CI (`.github/workflows/deploy-bundle.yml`)**:
   - Acionado apenas em `push` para `main` (após merge via PR).
   - Executa `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked`, `python3 -m unittest discover -s deploy/vps -p 'test_*.py' -v` e `cargo audit`.
   - Compila o binário release em `ubuntu-24.04`, empacota `bin/qrcardapio`, `static/` e `release.json` em `qrcardapio-<sha12>-linux-amd64.tar.gz`, gera atestação criptográfica de proveniência (`actions/attest-build-provenance`) com bundle Sigstore offline e publica na release `deploy-main` enviando `manifest.json` por último.
2. **Agente pull na VPS (`deploy/vps/qrcardapio-deploy-agent.py`)**:
   - Executado a cada 60 segundos por `qrcardapio-deploy.timer` (com `flock` e trava de segurança `ConditionPathExists=/etc/qrcardapio/auto-deploy.enabled`).
   - Consulta a release pública `deploy-main` usando `ETag` (`304 Not Modified` não consome limite de API).
   - Valida que `manifest.json` aponta para o HEAD atual de `refs/heads/main`, verifica SHA-256 exato e valida a assinatura offline com `gh attestation verify --bundle ... --signer-workflow ... --source-ref refs/heads/main --source-digest <sha> --deny-self-hosted-runners`.
   - Extrai o pacote com proteção contra path traversal e symlinks para `/opt/qrcardapio/releases/<sha12>`, testa `--help`, gera backup online do SQLite (`db backup` via `VACUUM INTO` em `/var/backups/qrcardapio/`), troca atomicamente o symlink `/opt/qrcardapio/current`, reinicia apenas `qrcardapio.service` (sem tocar no Caddy ou em outros projetos), valida `/healthz`, `/readyz`, páginas SEO e descoberta MCP, e executa `republish-all`.
   - **Rollback automático**: se o reinício ou qualquer verificação falhar, o agente restaura imediatamente o symlink `/opt/qrcardapio/current` para a release anterior, reinicia `qrcardapio.service`, confirma recuperação e grava o SHA com falha para evitar loop de reinício.

### Cardápio público de demonstração

O comando `serve` prepara o exemplo antes de abrir a porta HTTP. O pacote assinado já leva o catálogo e as fotos otimizadas dentro do binário, prontas para publicação. O passo de prontidão do agente só passa depois de o cardápio estar gravado em `DATA_DIR/published/bistro-da-praca/`. A rotina usa o mesmo formato estático dos cardápios comuns e pode executar de novo sem criar contas, estabelecimentos ou cobranças. `seed-public-demo` reconstitui o exemplo pela CLI, usando o mesmo ambiente do serviço.

O QR da arte já imprime `https://qrcardapio.com.br/#demonstracao`. A home encaminha esse fragmento pelo navegador para `/c/bistro-da-praca` e a placa também abre o cardápio por clique. A imagem SVG deve permanecer a mesma. A reserva recusa novos endereços personalizados com esse slug. Se uma base antiga já contém um estabelecimento com esse endereço, a rotina interrompe a inicialização antes de escrever no diretório publicado. O agente preserva a release anterior via rollback.

### Controles operacionais na VPS

- **Pausar autodeploy (kill-switch)**: `rm -f /etc/qrcardapio/auto-deploy.enabled`
- **Reativar autodeploy**: `touch /etc/qrcardapio/auto-deploy.enabled`
- **Executar imediatamente**: `systemctl start qrcardapio-deploy.service && journalctl -u qrcardapio-deploy.service -n 50 --no-pager`
- **Rollback manual instantâneo**: `ln -sfn /opt/qrcardapio/releases/<sha-anterior> /opt/qrcardapio/current && systemctl restart qrcardapio.service`

## Commercial operations

All commands require an existing platform admin (`qrcardapio admin create`) and append an audit event to `domain_events` in the same transaction.

```bash
# Explicit grant of a module or recurring add-on (trial <= 31 days, one per offer; courtesy/beta/internal_preview <= 366 days)
qrcardapio operator grant --operator admin@example.com --slug bar-k7m2 --offer menu --kind courtesy --days 30 --reason "pilot"
# Revoke (other sources of the same capability, such as a paid contract, stay active)
qrcardapio operator revoke --operator admin@example.com --slug bar-k7m2 --grant-id <uuid>
# Suspend new protected operations (publishing); published menu, data, unpublish and billing stay available
qrcardapio operator suspend --operator admin@example.com --slug bar-k7m2 --kind security --reason "phishing report"
qrcardapio operator reinstate --operator admin@example.com --slug bar-k7m2 --reason "resolved"
```

Planned modules (`delivery`, `tables`, …) cannot be granted: the lifecycle refuses it. Launching a module or changing a price is a code change in `contracts/commercial/` ([MODULE_CATALOG](../business/MODULE_CATALOG.md#changing-the-catalog)); the binary refuses to start if a sold version was edited or a lifecycle moved illegally — fix the contract, never the registry tables.

Useful read-only checks:

```bash
sqlite3 /data/db/qrcardapio.sqlite "SELECT id, origin, cycle_code, total_centavos, activated_at FROM commercial_contracts WHERE establishment_id = '<id>' ORDER BY created_at DESC;"
sqlite3 /data/db/qrcardapio.sqlite "SELECT seq, event_type, aggregate_id, occurred_at FROM domain_events WHERE establishment_id = '<id>' ORDER BY seq;"
```

`republish-all` (run by the deploy agent) re-renders only menus without pending draft changes; establishments with unpublished edits keep their live snapshot until the owner publishes.

Dashboard publication and `republish-all` share per-establishment lock files in `DATA_DIR/published/.tmp/publish-locks/`. The filesystem releases the locks when a process exits. Preserve that directory while the service or a publication command is running.

## Rollback of the commercial foundation release

Releases from this one on run migrations with `ignore_missing = true`, so later rollbacks to them are a symlink switch. Binaries released **before** it reject the new migration `20261124000009` (`VersionMissing`) and will not start after an automatic rollback. The new tables are additive, so the safe manual procedure is:

1. Stop the service and take a backup: `qrcardapio db backup --output /var/backups/qrcardapio/pre-rollback.sqlite`.
2. Remove only the migration bookkeeping row: `sqlite3 /data/db/qrcardapio.sqlite "DELETE FROM _sqlx_migrations WHERE version = 20261124000009;"`. Do not drop the new tables.
3. Point `/opt/qrcardapio/current` to the previous release and start it. It ignores the commercial tables; checkouts it creates have no contract yet.
4. On roll-forward the idempotent migration runs again and backfills contracts for any checkout created meanwhile (as `legacy_plan`, QR Menu v1, original price).

Alternative: restore the pre-deploy backup taken by the deploy agent (`/var/backups/qrcardapio/pre-<sha>-*.sqlite`), accepting the loss of writes made after the deploy, then replay missed Asaas webhooks from the Asaas dashboard.

## Payment providers (Stripe and Banco Inter)

Asaas, Stripe and Inter can be active at the same time ([ADR-011](../decisions/ADR-011-multi-provider-saas-billing.md), [providers](../architecture/payments/README.md)). All of them must use the same environment (`sandbox` or `production`). With Asaas and Stripe both configured, set `BILLING_CARD_PROVIDER=asaas|stripe`. Every provider requires an HTTPS `PUBLIC_BASE_URL` and `COOKIE_SECURE=true`. Variables are listed in `.env.example`.

### Stripe

1. Dashboard (Sandbox first) → Developers → API keys: create a **restricted key** (`rk_test_…`) with write access to Checkout Sessions and Subscriptions and read access to Invoices/Invoice Payments; set `STRIPE_SECRET_KEY`.
2. Workbench → Webhooks → add endpoint `https://<domain>/webhooks/stripe`, payload **snapshot**, API version **2026-11-18.endive**, events: `checkout.session.completed`, `checkout.session.expired`, `checkout.session.async_payment_succeeded`, `customer.subscription.created`, `customer.subscription.updated`, `customer.subscription.deleted`, `invoice.paid`, `charge.refunded`, `charge.dispute.created`, `charge.dispute.closed`. Copy the signing secret into `STRIPE_WEBHOOK_SECRET`.
3. Sandbox homologation: subscribe with card `4242 4242 4242 4242`; confirm the session, subscription and `invoice.paid` events (HTTP 200), coverage on `/dashboard/billing`, `SubscriptionActivated` in `domain_events`; cancel renewal (Stripe shows "cancels at period end"); refund the charge from the Dashboard and confirm publishing is blocked; resend an event from the Dashboard and confirm nothing changes.
4. Production: repeat with `rk_live_…`/`sk_live_…` and a live-mode endpoint (its own `whsec_`). Upgrading the API version means a new endpoint plus a code change of `stripe::API_VERSION`.

### Banco Inter

1. Sandbox: create an integration at developers.inter.co (community account), download certificate and key, note `client_id`/`client_secret`. Production: Internet Banking PJ → Integrar → Nova integração with scopes `boleto-cobranca.read` and `boleto-cobranca.write` (analysis required); the certificate lasts 1 year (sandbox: 30 days) and is renewed 90 days before expiry with the same client id and secret.
2. Store the files readable only by the service user (for example `/etc/qrcardapio/inter.crt`, `/etc/qrcardapio/inter.key`, mode `0400`) and set `INTER_ENV`, `INTER_CLIENT_ID`, `INTER_CLIENT_SECRET`, `INTER_CERT_PATH`, `INTER_KEY_PATH`, `INTER_WEBHOOK_TOKEN` (32+ letters/digits) and, only for multi-account integrations, `INTER_ACCOUNT`. The server refuses to start if the PEM pair is invalid.
3. Use a **static egress IP** (Inter blocks some dynamic cloud ranges with an empty 406).
4. Register the callback once: `qrcardapio billing register-inter-webhook` (uses `PUBLIC_BASE_URL`; pass `--base https://<host>` for the dedicated mTLS host below).
5. Optional hardening: verify Inter's client certificate (Internet Banking → Minhas integrações → "Certificado Webhook", file `ca.crt`) on a dedicated host so browsers are never asked for a certificate, then register it with `--base`:

   ```caddyfile
   pagamentos.qrcardapio.com.br {
       tls {
           client_auth {
               mode require_and_verify
               trust_pool file /etc/caddy/inter-ca.crt
           }
       }
       handle /webhooks/inter/* {
           reverse_proxy app:8080
       }
       respond 404
   }
   ```

6. The server runs the Inter pass hourly (issue renewals 10 days ahead, reconcile open charges). Run it on demand with `qrcardapio billing run-inter-cycle`.
7. Sandbox homologation: save payer data, subscribe with "Pix ou boleto", confirm the charge appears in Inter and on `/dashboard/billing` (Pix QR, digitable line, PDF); simulate payment with `POST /cobranca/v3/cobrancas/{codigoSolicitacao}/pagar` (`{"pagarCom":"PIX"}`, sandbox only, mTLS); confirm the callback (or the next pass) grants coverage; cancel renewal and confirm pending charges become `CANCELADO`.

Refunds of Inter charges are manual (boleto has no refund API; Pix refunds need the end-to-end id). They are not reflected automatically — record the decision and keep the coverage consistent manually until the planned operator command exists.
