use axum::extract::Request;
use axum::http::header::{CACHE_CONTROL, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;

const CSP_HEADER: HeaderValue = HeaderValue::from_static(
    "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
);

pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut res = next.run(req).await;
    let headers = res.headers_mut();

    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        CSP_HEADER,
    );

    if (path.starts_with("/dashboard")
        || path.starts_with("/onboarding")
        || path.starts_with("/admin")
        || path == "/login"
        || path == "/register"
        || path == "/reset-password")
        && !headers.contains_key(CACHE_CONTROL)
    {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }

    res
}
