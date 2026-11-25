//! Errors shared by every payment provider adapter. Provider payloads and credentials are
//! never included in the error text (it reaches logs).

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("payment provider returned HTTP {0}")]
    Status(StatusCode),
    #[error("payment provider request could not be completed")]
    Transport,
    #[error("payment provider returned an invalid response")]
    Response,
}

impl GatewayError {
    pub fn rejected_checkout(&self) -> bool {
        matches!(
            self,
            Self::Status(
                StatusCode::BAD_REQUEST
                    | StatusCode::UNAUTHORIZED
                    | StatusCode::FORBIDDEN
                    | StatusCode::UNPROCESSABLE_ENTITY
                    | StatusCode::TOO_MANY_REQUESTS
            )
        )
    }
}
