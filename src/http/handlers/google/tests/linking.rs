use serde_json::json;

use super::fixture::*;
use crate::{
    application::auth::register_owner,
    i18n::Locale,
    infrastructure::{database::queries, security::PasswordService},
};

#[tokio::test]
async fn existing_account_requires_explicit_csrf_protected_link_and_same_session() {
    let (_dir, pool, app, provider) = setup(true, false).await;
    let auth = register_owner(
        &pool,
        &PasswordService::new(),
        EMAIL,
        PASSWORD,
        PASSWORD,
        Locale::PtBr,
    )
    .await
    .unwrap();
    let session = format!("{SESSION_COOKIE}={}", auth.raw_session_token);
    let flow = Flow::start(&app, "/auth/google/start", "").await;
    provider.authorize(&flow, "duplicate", json!({}), false);
    let conflict = flow.callback(&app, "code", "duplicate", "").await;
    assert!(cookie(&conflict, SESSION_COOKIE).is_none());
    let flash = browser_cookie(&conflict, FLASH_COOKIE);
    assert!(
        html(get(&app, "/login", &flash).await)
            .await
            .contains("google.link_required")
    );

    assert_eq!(
        post(&app, "/auth/google/link", &session, &[("_csrf", "invalid")])
            .await
            .status(),
        403
    );
    let start_link = || async {
        let result = post(
            &app,
            "/auth/google/link",
            &session,
            &[("_csrf", &auth.csrf_token)],
        )
        .await;
        assert_eq!(result.status(), 200);
        let browser = browser_cookie(&result, COOKIE_NAME);
        let markup = html(result).await;
        let url = field(&markup, "href=\"https://accounts.google.com").replace("&#38;", "&");
        let flow = Flow::new(&format!("https://accounts.google.com{url}"), browser);
        assert_eq!(flow.params["prompt"], "select_account");
        flow
    };
    let absent = start_link()
        .await
        .callback(&app, "code", "no-session", "")
        .await;
    assert_eq!(location(&absent), "/account/security");
    assert!(
        !queries::google::has_identity(&pool, &auth.account.id)
            .await
            .unwrap()
    );
    let wrong_email = start_link().await;
    provider.authorize(
        &wrong_email,
        "wrong-email",
        json!({"email":"someone@example.test"}),
        false,
    );
    let rejected = wrong_email
        .callback(&app, "code", "wrong-email", &session)
        .await;
    assert_eq!(location(&rejected), "/account/security");
    assert!(
        !queries::google::has_identity(&pool, &auth.account.id)
            .await
            .unwrap()
    );

    let flow = start_link().await;
    provider.authorize(&flow, "link-owner", json!({}), false);
    let linked = flow.callback(&app, "code", "link-owner", &session).await;
    assert_eq!(location(&linked), "/account/security");
    assert!(
        queries::google::has_identity(&pool, &auth.account.id)
            .await
            .unwrap()
    );
    let valid = Flow::start(&app, "/auth/google/start", "").await;
    provider.authorize(&valid, "login-owner", json!({}), false);
    assert!(
        cookie(
            &valid.callback(&app, "code", "login-owner", "").await,
            SESSION_COOKIE
        )
        .is_some()
    );
    assert_eq!(
        post(
            &app,
            "/account/google/unlink",
            &session,
            &[("_csrf", "invalid")]
        )
        .await
        .status(),
        403
    );
    let unlink = post(
        &app,
        "/account/google/unlink",
        &session,
        &[("_csrf", &auth.csrf_token)],
    )
    .await;
    assert_eq!(unlink.status(), 303);
    assert!(
        !queries::google::has_identity(&pool, &auth.account.id)
            .await
            .unwrap()
    );
    let password = post(
        &app,
        "/login",
        "",
        &[("email", EMAIL), ("password", PASSWORD)],
    )
    .await;
    assert!(cookie(&password, SESSION_COOKIE).is_some());
}
