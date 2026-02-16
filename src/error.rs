use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub(crate) struct ApiError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
    pub(crate) meta: Option<Value>,
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            meta: None,
        }
    }

    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn meta(mut self, meta: Value) -> Self {
        self.meta = Some(meta);
        self
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

pub(crate) fn error_payload(error: ApiError) -> Value {
    json!({
        "ok": false,
        "error": error.message,
        "code": error.code,
        "hint": error.hint,
        "meta": error.meta,
    })
}
