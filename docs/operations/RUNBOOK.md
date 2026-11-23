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

Depois de autorizar, volte ao cliente pelo link na tela de confirmação. Na homologação, peça para listar o rascunho, criar uma categoria com um item de exemplo e modificar o preço. Confira o rascunho no painel, publique se desejar e valide a URL pública. Revogue a conexão nas **Integrações de IA** e verifique que a sessão do cliente deixa de acessar o rascunho.

Para diagnosticar descoberta, consulte `/.well-known/oauth-protected-resource/mcp` e `/.well-known/oauth-authorization-server` na origem pública. Uma chamada sem credencial a `/mcp` deve retornar `401` com `WWW-Authenticate`. Se um cliente mostrar erro no callback, confira se usa a identidade publicada e a opção de autenticação OAuth indicada acima. O OAuth vincula credenciais à origem configurada. Alterações de domínio exigem nova conexão.
