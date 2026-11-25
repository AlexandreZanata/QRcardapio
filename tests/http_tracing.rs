#[path = "commercial/support.rs"]
mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::test]
async fn request_tracing_records_route_templates_without_tokens() {
    let harness = support::harness(false).await;
    let log = harness.dir.path().join("request-traces.log");
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_span_events(FmtSpan::NEW)
        .with_writer(std::fs::File::create(&log).unwrap())
        .finish();

    let callback = Request::post("/webhooks/inter/test-private-inter-token?secret=test-query")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let unmatched = Request::post("/webhooks/inter/test-unmatched-token/unexpected")
        .body(Body::empty())
        .unwrap();

    async {
        for request in [callback, unmatched] {
            let response = harness.app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
    .with_subscriber(subscriber)
    .await;

    let output = std::fs::read_to_string(log).unwrap();
    assert!(output.contains("/webhooks/inter/{token}"), "{output}");
    assert!(output.contains("<unmatched>"), "{output}");
    for secret in [
        "test-private-inter-token",
        "test-unmatched-token",
        "test-query",
    ] {
        assert!(!output.contains(secret), "{output}");
    }
}
