mod fixture;
mod linking;
mod mcp;

use std::sync::atomic::Ordering;

use chrono::Utc;
use serde_json::json;

use super::BROWSER_COOKIE;
use crate::{
    application::auth::create_password_reset_token, config::GoogleConfig,
    infrastructure::database::queries,
};
use fixture::*;

#[tokio::test]
async fn google_is_optional_and_requires_the_exact_secure_callback_origin() {
    let (_dir, _pool, app, _) = setup(false, false).await;
    for path in ["/login", "/register"] {
        assert!(
            !html(get(&app, path, "").await)
                .await
                .contains("google.continue")
        );
    }
    assert_eq!(get(&app, "/auth/google/start", "").await.status(), 404);
    assert_eq!(get(&app, "/auth/google/callback", "").await.status(), 404);
    assert!(GoogleConfig::new("", "sample", ORIGIN, true).is_err());
    assert!(GoogleConfig::new("id.apps.googleusercontent.com", "", ORIGIN, true).is_err());
    assert!(google_config(ORIGIN, false).is_err());
    assert!(google_config("http://test.example", false).is_err());
    assert!(google_config("https://test.example/path", true).is_err());
    assert_eq!(
        google_config("http://localhost:3000", false)
            .unwrap()
            .redirect_uri,
        "http://localhost:3000/auth/google/callback"
    );
    assert!(!format!("{:?}", google_config(ORIGIN, true).unwrap()).contains("fixture-value"));
}

#[tokio::test]
async fn google_signs_up_and_returns_by_subject_after_onboarding_and_logout() {
    let (_dir, pool, app, provider) = setup(true, false).await;
    for path in ["/login", "/register"] {
        let page = html(get(&app, path, "").await).await;
        assert!(page.contains("href=\"/auth/google/start\""));
        assert!(page.contains("data-i18n=\"google.continue\""));
    }
    let flow = Flow::start(
        &app,
        "/auth/google/start?return_to=https://unexpected.example",
        "",
    )
    .await;
    let expires_at: i64 = sqlx::query_scalar("SELECT expires_at FROM google_oauth_flows")
        .fetch_one(&pool)
        .await
        .unwrap();
    let lifetime = contract()["state_ttl_seconds"].as_i64().unwrap();
    assert!((lifetime - 5..=lifetime).contains(&(expires_at - Utc::now().timestamp())));
    provider.authorize(&flow, "first-code", json!({}), false);
    let response = flow.callback(&app, "code", "first-code", "").await;
    assert_eq!(response.status(), 303);
    assert_eq!(location(&response), "/onboarding");
    assert!(
        cookie(&response, SESSION_COOKIE)
            .unwrap()
            .contains("HttpOnly")
    );
    let session = browser_cookie(&response, SESSION_COOKIE);
    let replay = flow.callback(&app, "code", "first-code", "").await;
    assert_eq!(location(&replay), "/login");
    assert!(cookie(&replay, SESSION_COOKIE).is_none());
    let original = queries::find_account_by_email(&pool, EMAIL)
        .await
        .unwrap()
        .unwrap();
    assert!(original.password_hash.is_empty());
    assert_eq!(original.role, crate::domain::account::AccountRole::Owner);
    let page = html(get(&app, "/onboarding", &session).await).await;
    let response = post(
        &app,
        "/onboarding",
        &session,
        &[
            ("_csrf", csrf(&page)),
            ("name", "Coffee Fixture"),
            ("slug", "coffee-fixture"),
        ],
    )
    .await;
    assert_eq!(response.status(), 303);
    assert_eq!(location(&response), "/dashboard/menu");

    let security = html(get(&app, "/account/security", &session).await).await;
    assert!(security.contains("google.unlink_requires_password"));
    let locked = post(
        &app,
        "/account/google/unlink",
        &session,
        &[("_csrf", csrf(&page))],
    )
    .await;
    assert_eq!(locked.status(), 409);
    let logout = post(&app, "/logout", &session, &[("_csrf", csrf(&page))]).await;
    assert_eq!(logout.status(), 303);

    let again = Flow::start(&app, "/auth/google/start", &flow.browser).await;
    provider.authorize(
        &again,
        "returning-code",
        json!({"email":"updated@example.test",
        "email_verified":"true","iss":contract()["issuers"][1]}),
        false,
    );
    let returned = again.callback(&app, "code", "returning-code", "").await;
    assert_eq!(location(&returned), "/dashboard");
    let session = browser_cookie(&returned, SESSION_COOKIE);
    let dashboard = get(&app, "/dashboard", &session).await;
    assert_eq!(dashboard.status(), 200);
    assert!(
        queries::find_account_by_email(&pool, "updated@example.test")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(provider.exchanges.load(Ordering::Relaxed), 2);
    assert_eq!(provider.key_requests.load(Ordering::Relaxed), 1);

    // A Google-only account can add a password using the existing administrator-assisted recovery.
    let token = create_password_reset_token(&pool, EMAIL).await.unwrap();
    let reset = post(
        &app,
        "/reset-password",
        "",
        &[
            ("token", &token),
            ("new_password", PASSWORD),
            ("confirm_password", PASSWORD),
        ],
    )
    .await;
    assert_eq!(reset.status(), 303);
    let result = post(
        &app,
        "/login",
        "",
        &[("email", EMAIL), ("password", PASSWORD)],
    )
    .await;
    assert_eq!(result.status(), 303);
    assert!(cookie(&result, SESSION_COOKIE).is_some());
}

#[tokio::test]
async fn signed_google_claims_wrong_nonce_wrong_browser_and_replay_cannot_create_sessions() {
    let (_dir, pool, app, provider) = setup(true, false).await;
    let cross_browser = Flow::start(&app, "/auth/google/start", "").await;
    let stolen = Flow {
        browser: format!("{BROWSER_COOKIE}={}", "x".repeat(43)),
        params: cross_browser.params.clone(),
    };
    let callback = stolen.callback(&app, "code", "no-exchange", "").await;
    assert_eq!(location(&callback), "/login");
    assert!(cookie(&callback, SESSION_COOKIE).is_none());
    assert_eq!(provider.exchanges.load(Ordering::Relaxed), 0);

    let cancelled = cross_browser
        .callback(&app, "error", "access_denied", "")
        .await;
    let flash = browser_cookie(&cancelled, FLASH_COOKIE);
    assert!(
        html(get(&app, location(&cancelled), &flash).await)
            .await
            .contains("google.cancelled")
    );
    let replay = cross_browser
        .callback(&app, "code", "no-exchange", "")
        .await;
    assert_eq!(location(&replay), "/login");
    assert_eq!(provider.exchanges.load(Ordering::Relaxed), 0);

    let expired = Flow::start(&app, "/auth/google/start", "").await;
    sqlx::query("UPDATE google_oauth_flows SET expires_at = 0")
        .execute(&pool)
        .await
        .unwrap();
    let callback = expired.callback(&app, "code", "no-exchange", "").await;
    assert_eq!(location(&callback), "/login");
    assert!(cookie(&callback, SESSION_COOKIE).is_none());
    assert_eq!(provider.exchanges.load(Ordering::Relaxed), 0);

    let cases = [
        (json!({"nonce":"stale"}), false, "google.invalid_identity"),
        (
            json!({"aud":"different.apps.googleusercontent.com"}),
            false,
            "google.invalid_identity",
        ),
        (
            json!({"azp":"different.apps.googleusercontent.com"}),
            false,
            "google.invalid_identity",
        ),
        (
            json!({"iss":"https://example.test"}),
            false,
            "google.invalid_identity",
        ),
        (json!({"exp":1}), false, "google.invalid_identity"),
        (
            json!({"email_verified":false}),
            false,
            "google.email_unverified",
        ),
        (json!({}), true, "google.invalid_identity"),
    ];
    for (i, (claims, signature, error)) in cases.into_iter().enumerate() {
        let flow = Flow::start(&app, "/auth/google/start", "").await;
        let code = format!("invalid-{i}");
        provider.authorize(&flow, &code, claims, signature);
        let response = flow.callback(&app, "code", &code, "").await;
        assert_eq!(location(&response), "/login");
        assert!(cookie(&response, SESSION_COOKIE).is_none());
        let flash = browser_cookie(&response, FLASH_COOKIE);
        assert!(
            html(get(&app, "/login", &flash).await)
                .await
                .contains(error)
        );
    }
    assert!(
        queries::find_account_by_email(&pool, EMAIL)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn google_keeps_the_selected_paid_plan_through_signup() {
    let (_dir, _pool, app, provider) = setup(true, true).await;
    let page = html(get(&app, "/register?plan=annual", "").await).await;
    assert!(page.contains("/auth/google/start?plan=annual"));
    let flow = Flow::start(&app, "/auth/google/start?plan=annual", "").await;
    provider.authorize(&flow, "plan-code", json!({}), false);
    let signup = flow.callback(&app, "code", "plan-code", "").await;
    assert_eq!(location(&signup), "/onboarding?plan=annual");
    let session = browser_cookie(&signup, SESSION_COOKIE);
    let page = html(get(&app, location(&signup), &session).await).await;
    let result = post(
        &app,
        "/onboarding?plan=annual",
        &session,
        &[
            ("_csrf", csrf(&page)),
            ("name", "Annual Cafe"),
            ("slug", "annual-cafe"),
        ],
    )
    .await;
    assert_eq!(location(&result), "/dashboard/billing?plan=annual");
}
