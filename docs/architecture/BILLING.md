# Assinatura QR Cardápio com Asaas

Um estabelecimento pode contratar um plano mensal de **R$ 29,90** por cartão de crédito. A página pública e o painel exibem o preço a partir de `MONTHLY_PRICE_CENTAVOS`. O QR Cardápio monta a cobrança em reais com aritmética inteira. O Asaas hospeda o checkout e coleta os dados do pagador e do cartão.

## Acesso e renovação

- O proprietário escolhe o Plano Essencial na landing e inicia a assinatura pelo painel após entrar na conta e cadastrar o estabelecimento. Gerentes podem acompanhar o status. A contratação exige configuração do Asaas. Ambientes HTTP com `localhost` ou IP privado ou de loopback exibem uma prévia visual quando a cobrança está desabilitada.
- Quem edita o cardápio pode salvar e visualizar rascunhos. Quando a cobrança está habilitada, tanto a primeira publicação quanto as atualizações exigem que um pagamento integral cubra a data atual.
- O início do período é o vencimento original da cobrança. O acesso vence no mesmo dia do mês seguinte, ajustado ao último dia quando necessário. O vencimento é exclusivo e o calendário usa UTC−03:00. A confirmação de um novo pagamento estende a cobertura conforme o respectivo vencimento.
- O cancelamento pelo painel encerra a recorrência no Asaas. Um período pago continua disponível até o vencimento. Um estorno integral ou chargeback invalida a cobertura da cobrança afetada. Acesso já coberto por outra cobrança segue o respectivo período.
- O cardápio publicado continua no ar até ser retirado pelo estabelecimento. A URL pública e o QR Code permanecem estáveis, com entrega estática pelo Caddy ou pelo fallback Axum. Retirar o cardápio do ar continua disponível a qualquer momento.

## Sincronização

1. O `POST /dashboard/billing/checkout` reserva um identificador local antes de chamar `POST /v3/checkouts`. O índice único parcial impede a abertura concorrente de outro checkout no mesmo estabelecimento e ambiente. O checkout fica disponível por 30 minutos. O painel oferece um link direto ao domínio Asaas depois de persistir o ID retornado.
2. O retorno do navegador abre o painel. `CHECKOUT_CREATED` pode recuperar uma reserva após timeout usando `externalReference` ou o identificador na URL de retorno. A reserva sem ID nunca apresentou um link e pode ser refeita após 60 minutos. Chegadas de `CHECKOUT_CANCELED` ou `CHECKOUT_EXPIRED` fecham um checkout em aberto.
3. O webhook associa a assinatura ao checkout pelo `checkoutSession` recebido na assinatura ou na cobrança. Se `CHECKOUT_PAID` chegar sem essa associação, o proprietário pode solicitar uma conciliação pontual que consulta `GET /v3/payments?checkoutSession=...` e vincula a assinatura retornada. A cobertura usa as cobranças conciliadas por eventos de pagamento, inclusive quando o evento da cobrança chegou antes do vínculo.
4. A validação e a aplicação de cada evento suportado acontecem em uma transação SQLite. A chave `(environment, event_id)` deduplica reentregas. A ordenação de eventos financeiros se dá pela data fornecida pelo Asaas para a mesma cobrança. No mesmo segundo, uma reversão prevalece sobre uma confirmação.

O webhook exige `asaas-access-token` próprio, separado da chave de API, com comparação em tempo constante. O cliente HTTP usa HTTPS, destinos fixos por ambiente, timeout e política sem redirecionamentos nem retentativa automática. As chaves e os payloads do provedor ficam fora dos logs. A integração persiste identificadores opacos, datas e situações, sem dados de cartão ou cadastro fiscal.

[Abrir documentação oficial do checkout recorrente](https://docs.asaas.com/docs/checkout-com-assinatura-recorrente) · [Listar cobranças por checkout](https://docs.asaas.com/reference/list-payments) · [Eventos e confirmação de cobrança](https://docs.asaas.com/docs/webhook-para-cobrancas) · [Remover assinatura](https://docs.asaas.com/reference/remover-assinatura)
