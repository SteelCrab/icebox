use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    MissingApiKey,
    Http(reqwest::Error),
    Api {
        status: reqwest::StatusCode,
        message: Option<String>,
        body: String,
        retryable: bool,
        retry_after_secs: Option<u64>,
    },
    Json(serde_json::Error),
    RetriesExhausted {
        attempts: u32,
        last_error: Box<ApiError>,
    },
}

impl ApiError {
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Api { retryable, .. } => *retryable,
            Self::Http(e) => e.is_timeout() || e.is_connect(),
            _ => false,
        }
    }

    #[must_use]
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(status.as_u16()),
            _ => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(
                f,
                "No API key. Set ANTHROPIC_API_KEY or run `icebox login`."
            ),
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::Api {
                status,
                message,
                body,
                ..
            } => {
                write!(f, "API error ({status})")?;
                if let Some(msg) = message {
                    write!(f, ": {msg}")?;
                }
                if status.as_u16() == 401 {
                    write!(
                        f,
                        "\n  Auth failed. Run `icebox whoami` to check credentials."
                    )?;
                }
                if status.as_u16() == 429 {
                    write!(f, "\n  Rate limited. Please wait a moment and try again.")?;
                }
                if !body.is_empty() && message.is_none() {
                    let short: String = body.chars().take(200).collect();
                    write!(f, ": {short}")?;
                }
                Ok(())
            }
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::RetriesExhausted {
                attempts,
                last_error,
            } => write!(
                f,
                "retries exhausted after {attempts} attempts: {last_error}"
            ),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
