# Cadastro e login com o Google

O QR Cardápio usa OpenID Connect com OAuth 2.0 Authorization Code e PKCE S256. A integração solicita `openid email` e recebe o código no servidor. O Google confirma a identidade em um ID token. O QR Cardápio valida o token, cria a própria sessão e descarta os tokens do provedor.

## Fluxo

1. O navegador abre `/auth/google/start` pela tela de cadastro ou de login. O servidor limita os inícios por IP e gera `state`, `nonce` e verificador PKCE independentes. O cookie do navegador é `HttpOnly`, `SameSite=Lax` e tem `Secure` em HTTPS. O estado expira em dez minutos e só pode ser consumido uma vez pelo navegador que iniciou o fluxo.
2. Em `/auth/google/callback`, o servidor troca o código pelo ID token com o verificador, as credenciais do cliente e a URI de retorno exata. A assinatura RS256 usa o JWKS oficial do Google, mantido em cache conforme `Cache-Control`. O token precisa incluir emissor e destinatário válidos, datas dentro da janela esperada, o `nonce` original, um `sub` e um e-mail que o Google sinaliza como verificado.
3. O primeiro acesso cria uma conta de proprietário quando o e-mail está disponível. Nos próximos acessos, o `sub` identifica a mesma conta mesmo se o Google informar um e-mail novo. A criação leva à configuração do estabelecimento. A escolha de plano acompanha a sessão até a etapa de contratação. O retorno ao MCP preserva o consentimento inicial e pede a criação do estabelecimento quando necessária.
4. Usuários com senha entram pela tela atual e vinculam o Google em **Segurança da conta**. O vínculo exige a sessão original, CSRF e o mesmo e-mail. A opção de desvincular exige uma senha local. O suporte pode emitir um link de recuperação para um usuário Google que precise cadastrar uma senha, seguindo o processo atual de verificação de identidade.

O e-mail que o Google atesta serve para checar o cadastro ou o vínculo. As regras de verificação de identidade do suporte continuam válidas, inclusive para endereços de terceiros que o Google não administra. O vínculo e as permissões locais são revogáveis pelo QR Cardápio.

## Validação

O contrato HTTP está em [contracts/auth/google.json](../../contracts/auth/google.json). A suíte chama o roteador e intercepta somente o transporte externo, avaliando a requisição real de troca, o desafio PKCE e a assinatura em uma chave RSA gerada para o teste. Os demais passos usam sessões, migrações e consultas reais do aplicativo.

```bash
cargo test --locked --lib http::handlers::google
cargo test --locked --test mcp
```

A [configuração e homologação no Google Cloud](../operations/RUNBOOK.md#habilitação-do-login-com-o-google) verifica as credenciais, o domínio e a tela de consentimento junto ao provedor.

[OpenID Connect](https://developers.google.com/identity/openid-connect/openid-connect) · [Referência de claims](https://developers.google.com/identity/openid-connect/reference) · [Verificação do ID token](https://developers.google.com/identity/sign-in/web/backend-auth) · [OAuth para aplicativos web](https://developers.google.com/identity/protocols/oauth2/web-server)
