#[path = "mcp/support.rs"]
mod support;

use axum::http::{StatusCode, header};
use reqwest::Url;
use serde_json::json;

use qrcardapio::infrastructure::database::queries::list_categories_for_est;
use support::*;

#[tokio::test]
async fn oauth_and_mcp_keep_ai_menu_edits_tenant_scoped_and_unpublished() {
    let (_dir, pool, app) = setup(true).await;
    let (first_owner, est) = owner(&pool, "test-cafe").await;
    let session = format!("qr_session={}", first_owner.raw_session_token);
    let published = form(
        &app,
        "/dashboard/publish",
        &[("_csrf", &first_owner.csrf_token)],
        &session,
    )
    .await;
    assert_eq!(published.status(), StatusCode::SEE_OTHER);
    let before = request(&app, "GET", "/c/test-cafe", &[], String::new()).await;
    assert_eq!(before.status(), StatusCode::OK);
    let before = body(before).await;
    let (other, other_est) = owner(&pool, "other-cafe").await;
    let unauthorized = rpc(
        &app,
        "invalid",
        "initialize",
        json!({"protocolVersion":"2025-11-25"}),
    )
    .await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert!(
        unauthorized.headers()[header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap()
            .contains("/.well-known/oauth-protected-resource/mcp")
    );
    let metadata = request(
        &app,
        "GET",
        "/.well-known/oauth-protected-resource/mcp",
        &[],
        String::new(),
    )
    .await;
    assert_eq!(json_body(metadata).await["resource"], RESOURCE);

    let code = authorize(&app, CHATGPT, None, "menu.read menu.write").await;
    assert_eq!(
        code_exchange(
            &app,
            CHATGPT,
            &code,
            "IncorrectVerifierThatStillHasSufficientLength123"
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let credentials = json_body(code_exchange(&app, CHATGPT, &code, VERIFIER).await).await;
    let access = credentials["access_token"].as_str().unwrap();
    assert_eq!(
        code_exchange(&app, CHATGPT, &code, VERIFIER).await.status(),
        StatusCode::BAD_REQUEST
    );
    let init = json_body(
        rpc(
            &app,
            access,
            "initialize",
            json!({"protocolVersion":"2025-11-25"}),
        )
        .await,
    )
    .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    let modern = request(
        &app,
        "POST",
        "/mcp",
        &[
            ("content-type", "application/json"),
            ("authorization", &format!("Bearer {access}")),
            ("mcp-protocol-version", "2026-07-28"),
            ("mcp-method", "server/discover"),
        ],
        json!({"jsonrpc":"2.0","id":2,"method":"server/discover",
        "params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28"}}})
        .to_string(),
    )
    .await;
    assert_eq!(
        json_body(modern).await["result"]["supportedVersions"][0],
        "2026-07-28"
    );
    let catalog = json_body(rpc(&app, access, "tools/list", json!({})).await).await;
    assert_eq!(catalog["result"]["tools"].as_array().unwrap().len(), 2);

    let input = json!({"operation_key":"draft-first-1", "new_categories":[{"text":{"name":"Bebidas"},"products":[{"text":{"name":"Café coado"},"price_centavos":550}]}]});
    let first = tool(&app, access, "apply_menu_changes", input.clone()).await;
    assert_eq!(tool(&app, access, "apply_menu_changes", input).await, first);
    let category = first["structuredContent"]["applied"]["category_ids"][0]
        .as_str()
        .unwrap();
    let draft = tool(
        &app,
        access,
        "get_menu_draft",
        json!({"category_id":category}),
    )
    .await;
    let product = &draft["structuredContent"]["products"][0];
    let new_price = json!({"operation_key":"draft-price-1", "product_updates":[{"id":product["id"],"updated_at":product["updated_at"],"price_centavos":600}]});
    assert_eq!(
        tool(&app, access, "apply_menu_changes", new_price.clone()).await["structuredContent"]["applied"]
            ["products_updated"],
        1
    );
    let mut stale = new_price;
    stale["operation_key"] = json!("draft-stale-1");
    assert_eq!(
        tool(&app, access, "apply_menu_changes", stale).await["isError"],
        true
    );

    let foreign_code = authorize(&app, CLAUDE, Some(&other), "menu.read menu.write").await;
    let foreign = json_body(code_exchange(&app, CLAUDE, &foreign_code, VERIFIER).await).await;
    let foreign_access = foreign["access_token"].as_str().unwrap();
    let bad_batch = json!({"operation_key":"foreign-attempt", "new_categories":[{"text":{"name":"Should roll back"}}],
       "add_products":[{"category_id":category,"products":[{"text":{"name":"Extra"},"price_centavos":100}]}]});
    assert_eq!(
        tool(&app, foreign_access, "apply_menu_changes", bad_batch).await["isError"],
        true
    );
    assert!(
        list_categories_for_est(&pool, &other_est)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(list_categories_for_est(&pool, &est).await.unwrap().len(), 1);
    let live = request(&app, "GET", "/c/test-cafe", &[], String::new()).await;
    assert_eq!(live.status(), StatusCode::OK);
    assert_eq!(body(live).await, before);

    let refresh = credentials["refresh_token"].as_str().unwrap();
    let rotated = form(
        &app,
        "/mcp/oauth/token",
        &[
            ("grant_type", "refresh_token"),
            ("client_id", CHATGPT.0),
            ("refresh_token", refresh),
        ],
        "",
    )
    .await;
    assert_eq!(rotated.status(), StatusCode::OK);
    let refreshed = json_body(rotated).await;
    assert_eq!(
        rpc(&app, access, "tools/list", json!({})).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        form(
            &app,
            "/mcp/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("client_id", CHATGPT.0),
                ("refresh_token", refresh)
            ],
            ""
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        rpc(
            &app,
            refreshed["access_token"].as_str().unwrap(),
            "tools/list",
            json!({})
        )
        .await
        .status(),
        StatusCode::OK
    );

    let google = authorize(&app, GOOGLE, Some(&other), "menu.read").await;
    let google = json_body(code_exchange(&app, GOOGLE, &google, VERIFIER).await).await;
    let readonly = google["access_token"].as_str().unwrap();
    assert_eq!(
        rpc(
            &app,
            readonly,
            "tools/call",
            json!({"name":"apply_menu_changes","arguments":{}})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let session = format!("qr_session={}", other.raw_session_token);
    let integrations = request(
        &app,
        "GET",
        "/dashboard/integrations",
        &[("cookie", &session)],
        String::new(),
    )
    .await;
    let html = body(integrations).await;
    assert!(html.contains("Google Antigravity"));
    let grant: String = sqlx::query_scalar(
        "SELECT id FROM mcp_grants WHERE establishment_id = ? AND client_id = ?",
    )
    .bind(&other_est)
    .bind(GOOGLE.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        form(
            &app,
            "/dashboard/integrations/revoke",
            &[("connection", &grant), ("_csrf", &other.csrf_token)],
            &session
        )
        .await
        .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        rpc(&app, readonly, "tools/list", json!({})).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn disabled_route_and_invalid_callbacks_are_rejected() {
    let (_dir, _, app) = setup(false).await;
    assert_eq!(
        rpc(&app, "invalid", "initialize", json!({})).await.status(),
        StatusCode::NOT_FOUND
    );
    let (_dir, _, app) = setup(true).await;
    let url = Url::parse_with_params(
        &format!("{ORIGIN}/mcp/oauth/authorize"),
        [
            ("client_id", CHATGPT.0),
            ("redirect_uri", "https://example.test/other"),
            ("response_type", "code"),
            (
                "code_challenge",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
            ("code_challenge_method", "S256"),
        ],
    )
    .unwrap();
    let response = request(
        &app,
        "GET",
        &format!("{}?{}", url.path(), url.query().unwrap()),
        &[],
        String::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!response.headers().contains_key(header::LOCATION));
}
