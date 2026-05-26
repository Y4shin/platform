//! Internationalization primitives shared by every plugin and the host.
//!
//! Stage A (M14) introduces the [`Locale`] enum and the request-time / async
//! resolution helpers in [`LocaleResolver`]. Stage B builds the [`Message`],
//! [`Template`], `Domain`, and `Localizer` on top.
//!
//! The set of shipped locales is **closed at the SDK level** — adding a new
//! locale is a one-line SDK edit plus per-plugin `.po` files. Encoding it as an
//! enum (rather than a string) gives compile-time exhaustiveness, an O(1)
//! `as usize` array index for the upcoming catalog tables, and rules out the
//! `"en-US"` vs `"en_us"` typo class of bugs entirely.
//!
//! Locale resolution priority (per the M14 plan):
//! 1. The user's stored preference (`platform.user.locale`).
//! 2. The request's `Accept-Language` header — first variant we recognise.
//! 3. The deployment-wide default (`HostConfig.default_locale`, itself
//!    defaulting to [`Locale::En`]).

use axum::http::HeaderMap;

use crate::auth::User;

/// The set of locales the platform ships catalogs for. Closed enum: callers
/// that need to refer to a locale by code go through [`Locale::from_code`].
///
/// `Pseudo` is for CI only — the `junius i18n check` pseudo-locale pass
/// fails the build if any visible string is unwrapped. It must never be the
/// active locale in production.
#[repr(usize)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Locale {
    En = 0,
    De = 1,
    Pseudo = 2,
}

impl Locale {
    pub const COUNT: usize = 3;
    pub const ALL: [Locale; Self::COUNT] = [Self::En, Self::De, Self::Pseudo];

    /// Stable, lower-case BCP-47-ish code; used in `Accept-Language`, the
    /// stored `platform.user.locale` column, the `.po` filename, and the
    /// upcoming `UpdateLocale` RPC.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Pseudo => "pseudo",
        }
    }

    /// Parse a code into a `Locale`, accepting either the exact code or a
    /// language-only prefix of an `Accept-Language` tag (`en-US` → `Locale::En`).
    /// Returns `None` for unknown codes — callers fall through to the next
    /// resolution step.
    #[must_use]
    pub fn from_code(s: &str) -> Option<Self> {
        let lang = s.split(['-', '_']).next().unwrap_or(s);
        match lang.to_ascii_lowercase().as_str() {
            "en" => Some(Self::En),
            "de" => Some(Self::De),
            "pseudo" => Some(Self::Pseudo),
            _ => None,
        }
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::En
    }
}

/// Resolves the active locale for one piece of work, honouring the three-tier
/// priority order documented at the module level.
#[derive(Clone, Copy, Debug)]
pub struct LocaleResolver {
    default: Locale,
}

impl LocaleResolver {
    /// Build a resolver around a deployment default.
    #[must_use]
    pub const fn new(default: Locale) -> Self {
        Self { default }
    }

    /// The deployment-wide default this resolver was built with.
    #[must_use]
    pub const fn default_locale(&self) -> Locale {
        self.default
    }

    /// Resolve the locale for an in-flight HTTP request. Used by every
    /// session-scoped handler.
    #[must_use]
    pub fn resolve_for_request(&self, user: Option<&User>, headers: &HeaderMap) -> Locale {
        if let Some(u) = user
            && let Some(code) = u.locale.as_deref()
            && let Some(locale) = Locale::from_code(code)
        {
            return locale;
        }
        if let Some(accept) = headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            && let Some(locale) = parse_accept_language(accept)
        {
            return locale;
        }
        self.default
    }

    /// Resolve the locale for the bearer of a stored preference string, e.g.
    /// the value of `platform.user.locale` looked up at job-enqueue time.
    /// `None`/unrecognised falls back to the deployment default — this is the
    /// helper jobs and `.ics` feeds use, where there is no `Accept-Language`.
    #[must_use]
    pub fn resolve_for_stored(&self, stored: Option<&str>) -> Locale {
        stored.and_then(Locale::from_code).unwrap_or(self.default)
    }
}

/// Pick the first `Accept-Language` tag we recognise. Ignores q-values for v1
/// (we negotiate over a 3-locale set; ordering by appearance is sufficient).
fn parse_accept_language(header: &str) -> Option<Locale> {
    for entry in header.split(',') {
        let tag = entry.split(';').next().unwrap_or("").trim();
        if tag.is_empty() {
            continue;
        }
        if let Some(locale) = Locale::from_code(tag) {
            return Some(locale);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::*;
    use crate::auth::{User, UserId};

    fn user_with_locale(locale: Option<&str>) -> User {
        User {
            id: UserId(uuid::Uuid::nil()),
            email: "u@example.com".into(),
            display_name: "U".into(),
            locale: locale.map(String::from),
            memberships: vec![],
        }
    }

    #[test]
    fn from_code_handles_exact_and_language_only() {
        assert_eq!(Locale::from_code("en"), Some(Locale::En));
        assert_eq!(Locale::from_code("de"), Some(Locale::De));
        assert_eq!(Locale::from_code("pseudo"), Some(Locale::Pseudo));
        assert_eq!(Locale::from_code("en-US"), Some(Locale::En));
        assert_eq!(Locale::from_code("de_AT"), Some(Locale::De));
        assert_eq!(Locale::from_code("EN"), Some(Locale::En));
        assert_eq!(Locale::from_code("fr"), None);
        assert_eq!(Locale::from_code(""), None);
    }

    #[test]
    fn user_preference_wins_over_accept_language() {
        let resolver = LocaleResolver::new(Locale::En);
        let user = user_with_locale(Some("de"));
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        assert_eq!(
            resolver.resolve_for_request(Some(&user), &headers),
            Locale::De
        );
    }

    #[test]
    fn accept_language_used_when_no_user_pref() {
        let resolver = LocaleResolver::new(Locale::En);
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("fr-FR,de;q=0.8,en;q=0.5"),
        );
        // fr is unknown; skip to de.
        assert_eq!(resolver.resolve_for_request(None, &headers), Locale::De);
    }

    #[test]
    fn falls_back_to_default_when_nothing_matches() {
        let resolver = LocaleResolver::new(Locale::De);
        let headers = HeaderMap::new();
        assert_eq!(resolver.resolve_for_request(None, &headers), Locale::De);
    }

    #[test]
    fn user_with_unknown_locale_falls_through() {
        let resolver = LocaleResolver::new(Locale::En);
        let user = user_with_locale(Some("fr"));
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("de"),
        );
        assert_eq!(
            resolver.resolve_for_request(Some(&user), &headers),
            Locale::De
        );
    }

    #[test]
    fn resolve_for_stored_handles_async_paths() {
        let resolver = LocaleResolver::new(Locale::En);
        assert_eq!(resolver.resolve_for_stored(Some("de")), Locale::De);
        assert_eq!(resolver.resolve_for_stored(Some("de-AT")), Locale::De);
        assert_eq!(resolver.resolve_for_stored(Some("xx")), Locale::En);
        assert_eq!(resolver.resolve_for_stored(None), Locale::En);
    }
}
