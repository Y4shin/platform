//! Error and diagnostic types returned by the parser + validator.

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Failures resolving a `[config]` value or secret indirection.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SecretError {
    #[error("required config key {0:?} is missing")]
    MissingKey(String),
    #[error("config key {0:?} must be a string")]
    NotAString(String),
    #[error("secret reference uses an unsupported scheme {0:?}")]
    UnsupportedScheme(String),
    #[error("secret reference has an empty target (e.g. `env:` or `file:` with nothing after)")]
    EmptyTarget,
    #[error("environment variable {0:?} (referenced by config) is not set")]
    EnvMissing(String),
    #[error("could not read secret file {0:?}: {1}")]
    FileRead(std::path::PathBuf, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

/// A single problem found by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub severity: Severity,
    /// Stable machine-readable code (e.g. `"PLUGIN.NAME.INVALID"`).
    pub code: &'static str,
    /// Human-readable explanation.
    pub message: String,
    /// Dotted path to the offending field in the document.
    pub path: String,
}

/// Result of validating a manifest. May contain zero issues (valid),
/// warnings only (valid but flagged), or one or more errors (invalid).
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        self.issues.iter().all(|i| !i.severity.is_error())
    }

    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity.is_error())
    }

    pub(crate) fn error(
        &mut self,
        code: &'static str,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.issues.push(ValidationIssue {
            severity: Severity::Error,
            code,
            message: message.into(),
            path: path.into(),
        });
    }
}
