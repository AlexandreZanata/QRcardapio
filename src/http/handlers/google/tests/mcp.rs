use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::fixture::*;

#[tokio::test]
async fn a_google_signup_resumes_the_same_mcp_consent_after_establishment_setup() {
    let (_dir, _pool, app, provider) = setup(true, false).await;
    let mcp: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contracts/mcp/clients.json",
    )))
    .unwrap();
    let client = &mcp["clients"][0];
    let client_id = client["client_id"].as_str().unwrap();
    let redirect = client["redirect_uri"].as_str().unwrap();
    let verifier = "FixtureMcpCodeVerifierWithAtLeast43AsciiCharacters";
    let resource = format!("{ORIGIN}/mcp");
    let url = Url::parse_with_params(
        &format!("{ORIGIN}/mcp/oauth/authorize"),
        [
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("state", "mcp-client-state"),
            ("resource", &resource),
            ("scope", "menu.read"),
            ("code_challenge_method", "S256"),
            (
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier)),
            ),
        ],
    )
    .unwrap();
    let response = get(
        &app,
        &format!("{}?{}", url.path(), url.query().unwrap()),
        "",
    )
    .await;
    assert_eq!(response.status(), 303);
    let mcp_browser = browser_cookie(&response, "qr_mcp_browser");
    let consent = location(&response).to_owned();
    let page = html(get(&app, &consent, &mcp_browser).await).await;
    let start = field(&page, "href=\"/mcp/oauth/google/start?");
    assert_eq!(
        get(&app, &format!("/mcp/oauth/google/start?{start}"), "")
            .await
            .status(),
        422
    );

    let flow = Flow::start(
        &app,
        &format!("/mcp/oauth/google/start?{start}"),
        &mcp_browser,
    )
    .await;
    provider.authorize(&flow, "mcp-signup", json!({}), false);
    let signed_in = flow.callback(&app, "code", "mcp-signup", "").await;
    assert_eq!(location(&signed_in), consent);
    let local = browser_cookie(&signed_in, SESSION_COOKIE);
    let cookies = format!("{mcp_browser}; {local}");
    let response = get(&app, &consent, &cookies).await;
    assert_eq!(response.status(), 303);
    let onboarding = location(&response).to_owned();
    assert!(onboarding.starts_with("/onboarding?mcp="));
    let page = html(get(&app, &onboarding, &local).await).await;
    assert!(page.contains(&format!("action=\"{onboarding}\"")));
    let setup = post(
        &app,
        &onboarding,
        &local,
        &[
            ("_csrf", csrf(&page)),
            ("name", "MCP Cafe"),
            ("slug", "mcp-cafe"),
        ],
    )
    .await;
    assert_eq!(location(&setup), consent);
    let page = html(get(&app, &consent, &cookies).await).await;
    assert!(page.contains("MCP Cafe"));
    let completed = post(
        &app,
        "/mcp/oauth/consent",
        &cookies,
        &[
            ("_csrf", csrf(&page)),
            ("flow", consent.split_once("?flow=").unwrap().1),
            ("decision", "allow"),
        ],
    )
    .await;
    assert_eq!(completed.status(), 200);
    let page = html(completed).await;
    let redirect_with_params = field(&page, &format!("href=\"{redirect}?")).replace("&#38;", "&");
    let params = query(&Url::parse(&format!("{redirect}?{redirect_with_params}")).unwrap());
    assert_eq!(params["iss"], ORIGIN);
    assert_eq!(params["state"], "mcp-client-state");
    let token = post(
        &app,
        "/mcp/oauth/token",
        "",
        &[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("redirect_uri", redirect),
            ("resource", &resource),
            ("code_verifier", verifier),
            ("code", &params["code"]),
        ],
    )
    .await;
    assert_eq!(token.status(), 200);
    let credentials: Value = serde_json::from_str(&html(token).await).unwrap();
    assert_eq!(credentials["scope"], "menu.read");
    assert_eq!(credentials["token_type"], "Bearer");
}
