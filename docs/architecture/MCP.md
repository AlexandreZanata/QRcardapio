# Integrações de IA via MCP

O QR Cardápio serve `POST /mcp` por Streamable HTTP quando `MCP_ENABLED=true`. O cliente de IA importa o conector remoto e o proprietário autoriza o vínculo com sua conta no QR Cardápio. A integração oferece as ferramentas `get_menu_draft` e `apply_menu_changes`. Ela usa o modelo hospedado do aplicativo conectado e dispensa chaves de API de provedores no QR Cardápio.

## Ferramentas de cardápio

- `get_menu_draft` consulta categorias paginadas. Com `category_id`, retorna os produtos daquela categoria. `next_offset` permite continuar e `updated_at` identifica a versão de cada registro.
- `apply_menu_changes` aplica um lote de até 80 alterações. Pode criar categorias com produtos, adicionar produtos a categorias existentes e atualizar nome, descrição, traduções, visibilidade, disponibilidade e preço em centavos. As edições exigem a versão devolvida pela leitura. `operation_key` permite reentregar o mesmo lote por sete dias sem duplicar dados. Um identificador de outro estabelecimento, uma versão desatualizada ou um valor inválido cancela o lote inteiro.
- As ferramentas atuam nos rascunhos. O proprietário abre a prévia, revisa e publica pelo painel. O botão de publicação preserva as regras da assinatura. Visibilidade pode ser alterada no rascunho para retirar itens da próxima publicação.

A leitura de um cardápio pelo público continua estática, sem consulta de token ou de SQLite. O servidor MCP funciona sem sessão de transporte, com respostas JSON e descoberta moderna (`2026-07-28`) e inicialização nas revisões `2025-11-25`, `2025-06-18` e `2025-03-26`. A conexão inicial sem token recebe `401` com a localização dos metadados do recurso. O servidor não oferece um canal SSE independente, conforme a opção prevista pelo Streamable HTTP.

## OAuth

O recurso protegido é a origem `PUBLIC_BASE_URL` com `/mcp`. `/.well-known/oauth-protected-resource/mcp` informa o recurso e o servidor de autorização. `/.well-known/oauth-authorization-server` publica endpoints, PKCE S256, suporte a CIMD, o método público `none` no token endpoint e `iss` na resposta de autorização.

A lista de clientes e callbacks conhecidos fica em `src/domain/mcp.rs`. Ela usa as identidades publicadas por ChatGPT, Claude e Google Antigravity. A resolução é local. Novos clientes ou callbacks devem ser verificados contra a documentação e os metadados oficiais antes de entrar na lista. Não há registro dinâmico aberto nem consulta a URLs de cliente fornecidas na requisição.

O navegador inicia a autorização, faz login se precisar e confirma o acesso ao estabelecimento exibido. Na tela de conclusão, o link de retorno segue para o callback oficial preservando `state` e `iss`. O login reutiliza a autenticação e o limitador do QR Cardápio. O POST de consentimento valida `_csrf` e a solicitação associada ao navegador. O acesso é concedido a proprietários, com associação entre token, conta, estabelecimento, cliente e recurso, verificada em cada requisição.

`menu.read` permite leitura. `menu.write` permite edição e inclui leitura. Códigos de autorização são de uso único, válidos por dois minutos e vinculados ao callback e ao desafio PKCE. Tokens de acesso têm duração máxima de uma hora. O token de renovação é rotativo e tem validade máxima de 30 dias desde o vínculo. O painel permite revogar o acesso. Redefinir a senha revoga vínculos e códigos pendentes da conta. Apenas os hashes de códigos e tokens são persistidos.

[Streamable HTTP do MCP](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports) · [Versionamento moderno do MCP](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning) · [OAuth para MCP](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
