//! Internationalization primitives shared by every plugin and the host.
//!
//! Stage A introduces the [`Locale`] enum and [`LocaleResolver`]. Stage B
//! layers the catalog runtime on top: the [`Domain`] enum, the
//! [`TemplatePart`]/[`Template`] parsed-msgstr types, the [`Message`] trait
//! that codegen'd per-msgid structs implement, and the runtime
//! [`crate::localizer::Localizer`] that pulls templates from a fixed-shape
//! 2D `[Domain × Locale]` array — no hashmaps on the hot path.
//!
//! ## Why enums
//!
//! Both the locale set (closed at the SDK level) and the plugin-domain set
//! (closed at the host's build) are known at compile time. Encoding them as
//! enums (rather than strings) gives:
//!
//! - compile-time exhaustiveness in `match`,
//! - an `as usize` index into the catalog tables (zero-cost lookup),
//! - rejection of `"en-US"` vs `"en_us"` and `"events"` vs `"event"` typos.
//!
//! Adding a new locale is a one-line SDK edit + new `.po` files. Adding a new
//! plugin is a [`Domain`] variant edit (a future `junius sync` step will
//! regenerate this from `dev/platform.toml`; for M14 it is hand-written).
//!
//! ## Locale resolution priority
//!
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

// `Domain` is codegen'd by `junius sync` against the deployment's plugin
// set — see `crates/junius-sdk/src/generated/domains.rs`. Re-exported here
// so plugin code keeps importing it from `junius_sdk::i18n`.
pub use crate::generated::domains::Domain;

/// One parsed piece of a msgstr, produced by the build-time `.po` parser.
/// Stored as `&'static` data in the codegen'd per-locale catalog arrays.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TemplatePart {
    Literal(&'static str),
    /// Placeholder name as it appears between `{...}` in the msgid/msgstr.
    /// Validated at codegen against the message's typed-struct fields.
    Placeholder {
        name: &'static str,
    },
}

/// A msgstr after build-time parsing. Stored in the per-locale catalog array
/// and walked by the codegen'd `Message::render` impl.
#[derive(Copy, Clone, Debug)]
pub struct Template {
    pub parts: &'static [TemplatePart],
}

impl Template {
    /// Fallback template used when nothing else resolves. Kept as a unique
    /// `&'static Template` so the localizer never returns a heap allocation.
    pub const FALLBACK: Template = Template { parts: &[] };
}

/// Convenience: a unit `Template` value pointer the localizer falls back to
/// when no catalog row exists for the active or default locale. The render
/// impl in that case emits the empty string; the message's `KEY` is still
/// available for diagnostics. The `junius i18n check` gate keeps this from
/// happening in CI.
pub static FALLBACK_TEMPLATE: Template = Template::FALLBACK;

/// A statically-known catalog entry. The build-time codegen
/// (`junius_i18n_build::generate`) emits one implementing type per msgid;
/// `Localizer::t` is generic over it. No hand-written impls in user code.
pub trait Message {
    /// Owning domain — supplied by the build helper, which knows the plugin's
    /// domain name from its `Options.domain` field at codegen time.
    const DOMAIN: Domain;
    /// Stable, dense index assigned by codegen: the position of this msgid in
    /// the canonical (sorted) `en.po` ordering. Used as the inner index into
    /// the per-locale catalog slice.
    const ID: usize;
    /// Catalog key (the msgid), e.g. `"event.signup.subject"`. For diagnostics
    /// and the optional dynamic lookup path (e.g. translating an error code).
    const KEY: &'static str;
    /// Walk the parsed template, emitting literal text and typed substitutions.
    /// Implementations are generated; callers never write this.
    fn render(&self, template: &Template) -> String;
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
            user_roles: vec![],
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
