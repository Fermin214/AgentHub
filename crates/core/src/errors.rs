//! Stable codes for cross-domain failures, with diagnostics separate from UI copy.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct CoreError {
    pub code: String,
    pub params: Value,
    pub detail: String,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}
impl std::error::Error for CoreError {}

impl CoreError {
    pub fn from_error(error: anyhow::Error) -> Self {
        if let Some(coded) = error.downcast_ref::<Self>() {
            Self {
                code: coded.code.clone(),
                params: coded.params.clone(),
                detail: format!("{error:#}"),
            }
        } else {
            Self {
                code: "CORE_FAILURE".into(),
                params: serde_json::json!({}),
                detail: format!("{error:#}"),
            }
        }
    }
}

pub(crate) fn coded(code: &'static str, params: Value, detail: impl Into<String>) -> anyhow::Error {
    CoreError {
        code: code.into(),
        params,
        detail: detail.into(),
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_preserves_code_and_parameters_independently_of_diagnostic_wording() {
        let error = coded(
            "RECOVERY_REQUIRED",
            serde_json::json!({"path":"C:/fixture"}),
            "arbitrary diagnostic",
        )
        .context("request context");
        let wire = serde_json::to_value(CoreError::from_error(error)).unwrap();
        assert_eq!(wire["code"], "RECOVERY_REQUIRED");
        assert_eq!(wire["params"]["path"], "C:/fixture");
        assert!(wire["detail"]
            .as_str()
            .unwrap()
            .contains("arbitrary diagnostic"));
    }
}
