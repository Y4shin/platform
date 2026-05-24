//! Plugin-facing secret values.
//!
//! [`SecretString`] wraps a secret and redacts its `Debug` so it can't leak via
//! logging; read the value with [`SecretString::expose`]. [`SecretStore`] holds
//! a plugin's resolved secrets, keyed by the name declared in its manifest. The
//! host builds the store at boot; the macro-generated `Secrets` accessor reads
//! declared names out of it (an undeclared name has no generated method).

use std::collections::HashMap;
use std::sync::Arc;

/// A secret value with a redacted `Debug`. No `Display` — callers must
/// `expose()` deliberately.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The underlying secret. Use sparingly and never log the result.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

/// A plugin's resolved secrets, keyed by declared name. Cheap to clone.
#[derive(Clone, Default)]
pub struct SecretStore {
    inner: Arc<HashMap<String, SecretString>>,
}

impl SecretStore {
    #[must_use]
    pub fn new(secrets: HashMap<String, SecretString>) -> Self {
        Self {
            inner: Arc::new(secrets),
        }
    }

    /// Build from `(name, value)` pairs.
    pub fn from_pairs<I: IntoIterator<Item = (String, String)>>(pairs: I) -> Self {
        Self::new(
            pairs
                .into_iter()
                .map(|(k, v)| (k, SecretString::new(v)))
                .collect(),
        )
    }

    /// The secret named `name`, if present. The generated `Secrets` accessor
    /// only ever asks for declared names, which the host guarantees at boot.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SecretString> {
        self.inner.get(name)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = SecretString::new("hunter2");
        assert_eq!(format!("{s:?}"), "SecretString(***)");
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn store_lookup() {
        let store = SecretStore::from_pairs([("api_key".to_string(), "abc".to_string())]);
        assert_eq!(store.get("api_key").map(SecretString::expose), Some("abc"));
        assert!(store.get("nope").is_none());
    }
}
