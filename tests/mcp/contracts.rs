#[path = "contracts/protocols.rs"]
mod protocols;

use protocols::verify_protocols;

use axum::http::{StatusCode, header};
use serde::Deserialize;
use serde_json::json;

use super::support::*;

#[derive(Deserialize)]
struct Client {
    name: String,
    client_id: String,
    redirect_uri: String,
}

#[derive(Deserialize)]
struct Contract {
    endpoint: String,
    protected_resource: String,
    authorization_server: String,
    scopes: Vec<String>,
    legacy_protocol_versions: Vec<String>,
    modern_protocol_version: String,
    clients: Vec<Client>,
}

fn contract() -> Contract {
    serde_json::from_str(include_str!("../../contracts/mcp/clients.json")).unwrap()
}

#[tokio::test]
async fn chatgpt_connector_contract() {
    verify_client("ChatGPT").await;
}

#[tokio::test]
async fn claude_connector_contract() {
    verify_client("Claude").await;
}

#[tokio::test]
async fn antigravity_connector_contract() {
    verify_client("Google Antigravity").await;
}

async fn verify_client(name: &str) {
    let contract = contract();
    let client = contract
        .clients
        .iter()
        .find(|client| client.name == name)
        .unwrap();
    let (_dir, pool, app) = setup(true).await;
    let identity = (client.client_id.as_str(), client.redirect_uri.as_str());
    let (owner, est) = owner(&pool, "contract-cafe").await;

    let challenge = request(&app, "GET", &contract.endpoint, &[], String::new()).await;
    assert_eq!(
        challenge.status(),
        StatusCode::UNAUTHORIZED,
        "{}",
        client.name
    );
    let challenge = challenge.headers()[header::WWW_AUTHENTICATE]
        .to_str()
        .unwrap();
    assert!(challenge.contains(&format!("{ORIGIN}{}", contract.protected_resource)));
    assert!(challenge.contains(&format!("scope=\"{}\"", contract.scopes.join(" "))));
    assert!(
        !challenge.contains("error=\"invalid_token\""),
        "RFC 6750 §3.1: unauthenticated probe must not include invalid_token error"
    );

    let bad_token = rpc(&app, "invalid-token", "tools/list", json!({})).await;
    assert_eq!(bad_token.status(), StatusCode::UNAUTHORIZED);
    assert!(
        bad_token.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("error=\"invalid_token\"")
    );

    let client_origin = reqwest::Url::parse(&client.client_id)
        .unwrap()
        .origin()
        .ascii_serialization();
    let preflight = request(
        &app,
        "OPTIONS",
        &contract.endpoint,
        &[("origin", &client_origin)],
        String::new(),
    )
    .await;
    assert_eq!(preflight.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        preflight.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        client_origin
    );
    assert!(
        preflight.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS]
            .to_str()
            .unwrap()
            .contains("MCP-Protocol-Version")
    );
    let blocked_origin = request(
        &app,
        "OPTIONS",
        &contract.endpoint,
        &[("origin", "https://evil.example")],
        String::new(),
    )
    .await;
    assert_eq!(blocked_origin.status(), StatusCode::FORBIDDEN);
    let resource = json_body(
        request(
            &app,
            "GET",
            &contract.protected_resource,
            &[],
            String::new(),
        )
        .await,
    )
    .await;
    assert_eq!(
        resource["resource"],
        format!("{ORIGIN}{}", contract.endpoint)
    );
    assert_eq!(resource["authorization_servers"], json!([ORIGIN]));
    assert_eq!(resource["scopes_supported"], json!(contract.scopes));
    assert_eq!(resource["bearer_methods_supported"], json!(["header"]));
    let as_metadata = json_body(
        request(
            &app,
            "GET",
            &contract.authorization_server,
            &[],
            String::new(),
        )
        .await,
    )
    .await;
    assert_eq!(as_metadata["issuer"], ORIGIN);
    assert_eq!(
        as_metadata["authorization_endpoint"],
        format!("{ORIGIN}/mcp/oauth/authorize")
    );
    assert_eq!(
        as_metadata["token_endpoint"],
        format!("{ORIGIN}/mcp/oauth/token")
    );
    assert_eq!(
        as_metadata["authorization_response_iss_parameter_supported"],
        true
    );
    assert_eq!(as_metadata["client_id_metadata_document_supported"], true);
    assert_eq!(
        as_metadata["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    assert_eq!(
        as_metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    assert_eq!(
        as_metadata["grant_types_supported"],
        json!(["authorization_code", "refresh_token"])
    );

    let code = authorize(&app, identity, Some(&owner), &contract.scopes.join(" ")).await;
    let mismatched = form(
        &app,
        "/mcp/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", identity.0),
            ("redirect_uri", identity.1),
            ("code", &code),
            ("code_verifier", VERIFIER),
            ("resource", "https://other.example.test/mcp"),
        ],
        "",
    )
    .await;
    assert_eq!(
        mismatched.status(),
        StatusCode::BAD_REQUEST,
        "{}",
        client.name
    );
    assert_eq!(json_body(mismatched).await["error"], "invalid_target");
    let res = code_exchange(&app, identity, &code, VERIFIER).await;
    assert_eq!(res.status(), StatusCode::OK, "{}", client.name);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    let tokens = json_body(res).await;
    assert_eq!(tokens["scope"], contract.scopes.join(" "));
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(
        code_exchange(&app, identity, &code, VERIFIER)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let access = tokens["access_token"].as_str().unwrap();
    let delete_mcp = request(
        &app,
        "DELETE",
        &contract.endpoint,
        &[("authorization", &format!("Bearer {access}"))],
        String::new(),
    )
    .await;
    assert_eq!(delete_mcp.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(delete_mcp.headers()[header::ALLOW], "POST");
    verify_protocols(&app, &contract, access).await;

    let changes = json!({"operation_key":"contract-batch",
            "new_categories":[{"text":{"name":"Bebidas"},"products":[{"text":{"name":"Suco"},"price_centavos":990}]}]});
    let saved = tool(&app, access, "apply_menu_changes", changes.clone()).await;
    assert_eq!(saved["structuredContent"]["draft"], true);
    assert_eq!(saved["structuredContent"]["publication_required"], true);
    assert_eq!(
        saved,
        tool(&app, access, "apply_menu_changes", changes).await
    );
    let category = &saved["structuredContent"]["applied"]["category_ids"][0];
    let listed = tool(
        &app,
        access,
        "get_menu_draft",
        json!({"category_id":category}),
    )
    .await;
    assert_eq!(
        listed["structuredContent"]["products"][0]["price_centavos"],
        990
    );
    let product = &listed["structuredContent"]["products"][0];
    let updated = tool(&app, access, "apply_menu_changes", json!({
            "operation_key":"contract-update",
            "product_updates":[{"id":product["id"], "updated_at":product["updated_at"], "price_centavos":1090}]
        })).await;
    assert_eq!(
        updated["structuredContent"]["applied"]["products_updated"],
        1
    );
    assert_eq!(
        tool(
            &app,
            access,
            "get_menu_draft",
            json!({"establishment_id":est})
        )
        .await["isError"],
        true
    );
    assert_eq!(
        request(&app, "GET", "/c/contract-cafe", &[], String::new())
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let refresh = [
        ("grant_type", "refresh_token"),
        ("client_id", identity.0),
        ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
        ("resource", RESOURCE),
    ];
    let rotated = form(&app, "/mcp/oauth/token", &refresh, "").await;
    assert_eq!(rotated.status(), StatusCode::OK);
    let rotated = json_body(rotated).await;
    let fresh_access = rotated["access_token"].as_str().unwrap();
    assert_ne!(fresh_access, access);
    assert_eq!(
        rpc(&app, access, "tools/list", json!({})).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        form(&app, "/mcp/oauth/token", &refresh, "").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        rpc(&app, fresh_access, "tools/list", json!({}))
            .await
            .status(),
        StatusCode::OK
    );

    let grant: String = sqlx::query_scalar(
        "SELECT id FROM mcp_grants WHERE client_id = ? AND establishment_id = ?",
    )
    .bind(identity.0)
    .bind(&est)
    .fetch_one(&pool)
    .await
    .unwrap();
    let session = format!("qr_session={}", owner.raw_session_token);
    let revoke = form(
        &app,
        "/dashboard/integrations/revoke",
        &[("connection", &grant), ("_csrf", &owner.csrf_token)],
        &session,
    )
    .await;
    assert_eq!(revoke.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        rpc(&app, fresh_access, "tools/list", json!({}))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
