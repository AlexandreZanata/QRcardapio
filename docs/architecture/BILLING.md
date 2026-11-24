# Assinatura QR Cardápio com Asaas

O Plano Essencial tem três períodos de cobrança recorrente por cartão de crédito.

- Mensal de R$ 29,90.
- Semestral de R$ 170,43, com 5% de desconto sobre seis mensalidades.
- Anual de R$ 322,92, com 10% de desconto sobre doze mensalidades.

O domínio calcula os totais em centavos a partir de `MONTHLY_PRICE_CENTAVOS`. A landing e o painel usam esse catálogo. A reserva de checkout grava o preço e a duração contratados, inclusive quando o catálogo muda depois. O Asaas usa o ciclo `MONTHLY`, `SEMIANNUALLY` ou `YEARLY` e hospeda a coleta dos dados do pagador e do cartão.

## Acesso e renovação

- O proprietário escolhe o Plano Essencial na landing e inicia a assinatura pelo painel após entrar na conta e cadastrar o estabelecimento. Gerentes podem acompanhar o status. A contratação exige configuração do Asaas. Ambientes HTTP com `localhost` ou IP privado ou de loopback exibem uma prévia visual quando a cobrança está desabilitada.
- Quem edita o cardápio pode salvar e visualizar rascunhos. Quando a cobrança está habilitada, tanto a primeira publicação quanto as atualizações exigem que um pagamento integral cubra a data atual.
- O início do período é o vencimento original da cobrança. O acesso vence após 1, 6 ou 12 meses do vencimento, com ajuste para o último dia quando necessário. O pagamento precisa corresponder ao valor gravado no checkout do estabelecimento. O vencimento é exclusivo e o calendário usa UTC−03:00. A confirmação de um novo pagamento estende a cobertura conforme o respectivo vencimento.
- O cancelamento pelo painel encerra a recorrência no Asaas. Um período pago continua disponível até o vencimento. Um estorno integral ou chargeback invalida a cobertura da cobrança afetada. Acesso já coberto por outra cobrança segue o respectivo período.
- O cardápio publicado continua no ar até ser retirado pelo estabelecimento. A URL pública e o QR Code permanecem estáveis, com entrega estática pelo Caddy ou pelo fallback Axum. Retirar o cardápio do ar continua disponível a qualquer momento.

## Sincronização

1. O `POST /dashboard/billing/checkout` reserva um identificador local antes de chamar `POST /v3/checkouts`. O índice único parcial impede a abertura concorrente de outro checkout no mesmo estabelecimento e ambiente. O checkout fica disponível por 30 minutos. O painel oferece um link direto ao domínio Asaas depois de persistir o ID retornado.
2. O retorno do navegador abre o painel. `CHECKOUT_CREATED` pode recuperar uma reserva após timeout usando `externalReference` ou o identificador na URL de retorno. A reserva sem ID nunca apresentou um link e pode ser refeita após 60 minutos. Chegadas de `CHECKOUT_CANCELED` ou `CHECKOUT_EXPIRED` fecham um checkout em aberto.
3. O webhook associa a assinatura ao checkout pelo `checkoutSession` recebido na assinatura ou na cobrança. Se `CHECKOUT_PAID` chegar sem essa associação, o proprietário pode solicitar uma conciliação pontual que consulta `GET /v3/payments?checkoutSession=...` e vincula a assinatura retornada. A cobertura usa as cobranças conciliadas por eventos de pagamento, inclusive quando o evento da cobrança chegou antes do vínculo.
4. A validação e a aplicação de cada evento suportado acontecem em uma transação SQLite. A chave `(environment, event_id)` deduplica reentregas. A ordenação de eventos financeiros se dá pela data fornecida pelo Asaas para a mesma cobrança. No mesmo segundo, uma reversão prevalece sobre uma confirmação.

O webhook exige `asaas-access-token` próprio, separado da chave de API, com comparação em tempo constante. O cliente HTTP usa HTTPS, destinos fixos por ambiente, timeout e política sem redirecionamentos nem retentativa automática. As chaves e os payloads do provedor ficam fora dos logs. A integração persiste identificadores opacos, datas e situações, sem dados de cartão ou cadastro fiscal.

[Abrir documentação oficial do checkout recorrente](https://docs.asaas.com/docs/checkout-com-assinatura-recorrente) · [Listar cobranças por checkout](https://docs.asaas.com/reference/list-payments) · [Eventos e confirmação de cobrança](https://docs.asaas.com/docs/webhook-para-cobrancas) · [Remover assinatura](https://docs.asaas.com/reference/remover-assinatura)

## Commercial Engine (fase 1)

O Plano Essencial passou a ser o módulo **QR Menu v1** do [catálogo comercial](../business/MODULE_CATALOG.md). Os valores acima continuam idênticos e agora vêm de `contracts/commercial/` (não existe mais constante de preço no código).

- O checkout grava, na mesma transação da reserva, um contrato imutável em `commercial_contracts` com itens e capacidades (`commercial_contract_items`, `commercial_contract_capabilities`). `billing_checkouts.price_centavos` é o total da cotação. Detalhes: [COMMERCIAL_ENGINE](COMMERCIAL_ENGINE.md).
- O acesso à publicação é decidido pelo [Entitlement Engine](ENTITLEMENT_ENGINE.md) (`menu.publish`), a partir dos mesmos pagamentos confirmados pelo valor exato. A regra de cobertura é idêntica à visão `billing_coverage`.
- A primeira confirmação válida de cada contrato grava `SubscriptionActivated` no outbox, uma única vez, mesmo com reentrega ou eventos fora de ordem.
- Clientes existentes foram migrados para contratos `legacy_plan` sem nova cobrança (`tests/commercial_migration.rs`).
- Alterações de assinatura (adicionar módulos, trocar ciclo, proporcionalidade) não estão habilitadas; ver [SUBSCRIPTION_RULES](../business/SUBSCRIPTION_RULES.md).
- A homologação na conta Sandbox do Asaas continua pendente e é requisito para declarar o fluxo homologado.

## Provedores Stripe e Banco Inter

Desde [ADR-011](../decisions/ADR-011-multi-provider-saas-billing.md), o Asaas é um dos três provedores de cobrança SaaS. O cartão é processado pelo Asaas ou pela Stripe (`BILLING_CARD_PROVIDER`), e Pix/boleto pelo Banco Inter, com uma cobrança por ciclo emitida pela plataforma. Todos produzem os mesmos eventos normalizados (`GatewayEvent`) e a mesma regra de cobertura. Detalhes: [provedores](payments/README.md), [Stripe](payments/STRIPE.md), [Inter](payments/INTER.md).
