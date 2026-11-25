use std::collections::{BTreeMap, BTreeSet};

use axum::Router;
use axum::http::{Method, header};
use axum::response::Response;
use qrcardapio::application::auth::AuthSuccess;

use super::replay::{FAILING, admin_login, mark_dead};
use super::*;

const STATUS: &str = "/admin/outbox";
const RECOVERY: &str = "/admin/outbox/recovery";
const REPLAY: &str = "/admin/outbox/replay";
const FORM_MEDIA: &str = "application/x-www-form-urlencoded";

fn contract() -> Value {
    serde_json::from_str(include_str!("../../contracts/openapi/commerce.v1.json")).unwrap()
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn resolve<'a>(contract: &'a Value, entry: &'a Value) -> &'a Value {
    if let Some(reference) = entry["$ref"].as_str() {
        contract
            .pointer(reference.strip_prefix('#').unwrap())
            .unwrap()
    } else {
        entry
    }
}

fn assert_response(
    contract: &Value,
    path: &str,
    method: &str,
    response: &Response,
    expected: StatusCode,
) {
    assert_eq!(response.status(), expected, "{method} {path}");
    let specification = resolve(
        contract,
        &contract["paths"][path][method]["responses"][expected.as_str()],
    );
    for (name, expected) in [
        ("Cache-Control", "no-store"),
        ("X-Robots-Tag", "noindex, nofollow"),
    ] {
        let documented = resolve(contract, &specification["headers"][name]);
        assert_eq!(documented["schema"]["const"], expected);
        assert_eq!(response.headers()[name], expected);
    }
    if let Some(content_type) = response.headers().get(header::CONTENT_TYPE) {
        let media_type = content_type.to_str().unwrap().split(';').next().unwrap();
        assert!(
            specification["content"].get(media_type).is_some(),
            "undocumented {media_type} for {method} {path} {expected}"
        );
    } else {
        assert!(specification.get("content").is_none());
    }
    if expected == StatusCode::SEE_OTHER {
        let location = support::location(response);
        let schema = &resolve(contract, &specification["headers"]["Location"])["schema"];
        let documented = schema["const"] == location
            || schema["enum"]
                .as_array()
                .is_some_and(|values| values.contains(&json!(location)));
        assert!(
            documented,
            "undocumented redirect {location} for {method} {path}"
        );
    }
}

fn assert_diagnostics_object(body: &Value, schema: &Value) {
    let documented = keys(&schema["properties"]);
    let required: BTreeSet<_> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap())
        .collect();
    assert_eq!(documented, required);
    assert_eq!(keys(body), documented);
    assert_eq!(schema["additionalProperties"], false);
    for (key, value) in body.as_object().unwrap() {
        let kind = match value {
            Value::String(_) => "string",
            Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
            Value::Array(_) => "array",
            Value::Null => "null",
            _ => panic!("unexpected diagnostics JSON type for {key}"),
        };
        let documented_type = &schema["properties"][key]["type"];
        assert!(
            documented_type == kind
                || documented_type
                    .as_array()
                    .is_some_and(|kinds| kinds.contains(&json!(kind))),
            "wrong schema type for {key}"
        );
    }
}

async fn json_status(app: &Router, admin: &AuthSuccess, contract: &Value) -> Value {
    let response = support::get(app, admin, STATUS).await;
    assert_response(contract, STATUS, "get", &response, StatusCode::OK);
    let schema = resolve(
        contract,
        &contract["paths"][STATUS]["get"]["responses"]["200"]["content"]["application/json"]["schema"],
    );
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_diagnostics_object(&body, schema);
    for key in ["consumers", "dead_letters"] {
        let item_schema = resolve(contract, &schema["properties"][key]["items"]);
        for item in body[key].as_array().unwrap() {
            assert_diagnostics_object(item, item_schema);
            assert!(item.get("payload").is_none());
        }
    }
    body
}

async fn seed_dead_letter(pool: &SqlitePool) {
    append_events(pool, &[("one", "order", EventType::OrderCreated, true)]).await;
    dispatch_batch(pool, &[FAILING], Utc::now()).await.unwrap();
    mark_dead(pool).await;
}

#[tokio::test]
async fn outbox_openapi_matches_diagnostics_and_admin_access() {
    let contract = contract();
    assert_eq!(
        keys(&contract["paths"]),
        BTreeSet::from([STATUS, RECOVERY, REPLAY])
    );
    let security = &contract["components"]["securitySchemes"]["AdminSession"];
    assert_eq!(security["type"], "apiKey");
    assert_eq!(security["in"], "cookie");
    assert_eq!(security["name"], "qr_session");
    assert_eq!(contract["security"], json!([{ "AdminSession": [] }]));

    let harness = init().await;
    let app = &harness.app;
    let admin = admin_login(&harness.pool).await;
    let (owner, _) = support::owner(&harness.pool, "outbox-contract-owner").await;
    for (path, method) in [
        (STATUS, Method::GET),
        (RECOVERY, Method::GET),
        (REPLAY, Method::POST),
    ] {
        let verb = method.as_str().to_ascii_lowercase();
        assert_eq!(
            keys(&contract["paths"][path]),
            BTreeSet::from([verb.as_str()])
        );
        for (session, expected) in [
            (None, StatusCode::SEE_OTHER),
            (Some(&owner), StatusCode::FORBIDDEN),
        ] {
            let mut request = Request::builder().method(method.clone()).uri(path);
            if let Some(session) = session {
                request = request.header(
                    header::COOKIE,
                    format!("qr_session={}", session.raw_session_token),
                );
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_response(&contract, path, &verb, &response, expected);
            if expected == StatusCode::SEE_OTHER {
                assert_eq!(support::location(&response), "/login");
            }
        }
    }

    let empty = json_status(app, &admin, &contract).await;
    assert_eq!(
        empty,
        json!({"pending_events": 0, "oldest_pending_age_seconds": null, "consumers": [], "dead_letters": []})
    );
    seed_dead_letter(&harness.pool).await;
    let populated = json_status(app, &admin, &contract).await;
    assert_eq!(populated["pending_events"], 1);
    assert_eq!(populated["consumers"][0]["dead_letter_count"], 1);
    assert_eq!(populated["dead_letters"][0]["error_key"], "error.internal");
}

#[tokio::test]
async fn outbox_openapi_matches_the_browser_replay_form() {
    let contract = contract();
    let harness = init().await;
    let app = &harness.app;
    let admin = admin_login(&harness.pool).await;
    seed_dead_letter(&harness.pool).await;

    let recovery = support::get(app, &admin, RECOVERY).await;
    assert_response(&contract, RECOVERY, "get", &recovery, StatusCode::OK);
    let page = recovery.into_body().collect().await.unwrap().to_bytes();
    let page = std::str::from_utf8(&page).unwrap();
    let (_, form) = page
        .split_once(r#"method="post" action="/admin/outbox/replay">"#)
        .unwrap();
    let (form, _) = form.split_once("</form>").unwrap();
    let fields: BTreeMap<_, _> = form
        .lines()
        .filter_map(|line| {
            let (_, attributes) = line.split_once(r#"<input type="hidden" name=""#)?;
            let (name, value) = attributes.split_once(r#"" value=""#).unwrap();
            Some((name, value.split_once('"').unwrap().0))
        })
        .collect();
    let request_body = &contract["paths"][REPLAY]["post"]["requestBody"];
    let schema = resolve(&contract, &request_body["content"][FORM_MEDIA]["schema"]);
    let required: BTreeSet<_> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap())
        .collect();
    assert_eq!(fields.keys().copied().collect::<BTreeSet<_>>(), required);
    assert_eq!(keys(&schema["properties"]), required);
    assert_eq!(fields["_csrf"], admin.csrf_token);
    let options = format!(
        "consumer={}&event_id={}&generation={}",
        fields["consumer"], fields["event_id"], fields["generation"]
    );
    let limit = request_body["x-max-body-bytes"].as_u64().unwrap() as usize;
    let csrf_padding = "x".repeat(limit - options.len() - "&_csrf=".len());
    let at_limit = format!("{options}&_csrf={csrf_padding}");
    let valid = format!("{options}&_csrf={}", fields["_csrf"]);
    let wrong_media = app
        .clone()
        .oneshot(
            Request::post(REPLAY)
                .header(
                    header::COOKIE,
                    format!("qr_session={}", admin.raw_session_token),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_response(
        &contract,
        REPLAY,
        "post",
        &wrong_media,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    );

    for (body, expected) in [
        (format!("{at_limit}x"), StatusCode::PAYLOAD_TOO_LARGE),
        (at_limit, StatusCode::FORBIDDEN),
        (options, StatusCode::UNPROCESSABLE_ENTITY),
        (valid.clone(), StatusCode::SEE_OTHER),
        (valid, StatusCode::CONFLICT),
    ] {
        let response = support::post_form(app, &admin, REPLAY, body).await;
        assert_response(&contract, REPLAY, "post", &response, expected);
        if expected == StatusCode::SEE_OTHER {
            assert_eq!(support::location(&response), RECOVERY);
        }
    }
}
