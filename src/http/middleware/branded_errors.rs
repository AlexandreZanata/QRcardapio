use axum::extract::Request;
use axum::http::{Method, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::error::ErrorDetail;
use crate::http::middleware::locale::RequestLocale;
use crate::rendering::error_page::error_page;

pub async fn branded_errors(
    RequestLocale(locale): RequestLocale,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let page_request = request.method() != Method::HEAD
        && !path.starts_with("/static/")
        && !matches!(path, "/healthz" | "/readyz");
    let response = next.run(request).await;
    let status = response.status();
    let already_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"));
    if !page_request || already_html || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }

    let detail = response.extensions().get::<ErrorDetail>().map(|key| key.0);
    let (mut original, _) = response.into_parts();
    let (replacement, body) = error_page(locale, status, detail).into_parts();
    original.headers.remove(header::CONTENT_LENGTH);
    original.headers.extend(replacement.headers);
    Response::from_parts(original, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, http::StatusCode, routing::get};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn renders_error_art_with_original_status_and_hides_internal_details() {
        let router = Router::new()
            .route(
                "/fixture",
                get(|| async { crate::error::AppError::Internal("private diagnostic".into()) }),
            )
            .layer(axum::middleware::from_fn(branded_errors));
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/fixture?lang=en")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains("state-error.svg"));
        assert!(body.contains("Something went wrong"));
        assert!(!body.contains("private diagnostic"));
    }
}
