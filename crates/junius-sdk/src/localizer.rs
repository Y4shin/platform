//! The host-side `Localizer` and its request-scoped view.
//!
//! Catalogs are baked into the binary by the build-time codegen (see
//! `junius-i18n-build` and the `i18n_catalog!` macro). At boot, each plugin's
//! `register_i18n` calls into [`LocalizerBuilder::add_domain`], handing the
//! host a `[locale → catalog]` row keyed by [`Domain`]. The finished
//! [`Localizer`] is a fixed-shape 2D array — no hashmaps on the per-call path.

use std::sync::Arc;

use axum::http::HeaderMap;
use sqlx::PgPool;

use crate::auth::{User, UserId};
use crate::i18n::{Domain, FALLBACK_TEMPLATE, Locale, LocaleResolver, Message, Template};

/// Per-locale catalog table for one domain. Indexed by [`Message::ID`].
type LocaleCatalog = &'static [Option<&'static Template>];

/// Per-domain catalog row, indexed by [`Locale`] as `usize`.
type DomainRow = [LocaleCatalog; Locale::COUNT];

/// The empty row used as a default before plugins register. Every domain
/// slot must be replaced via [`LocalizerBuilder::add_domain`].
const EMPTY_ROW: DomainRow = [&[]; Locale::COUNT];

/// Builder for [`Localizer`]. Each plugin's `register_i18n` populates one
/// slot via [`Self::add_domain`].
#[derive(Clone)]
pub struct LocalizerBuilder {
    rows: [DomainRow; Domain::COUNT],
    default_locale: Locale,
}

impl LocalizerBuilder {
    /// Start with empty catalogs and the supplied deployment-wide fallback.
    #[must_use]
    pub fn new(default_locale: Locale) -> Self {
        Self {
            rows: [EMPTY_ROW; Domain::COUNT],
            default_locale,
        }
    }

    /// Install one domain's catalogs. Each `LocaleCatalog` is a `&'static`
    /// slice produced by the build-time codegen; this method does no work
    /// beyond writing the slot. Replacing an already-populated slot is
    /// allowed (the last writer wins — practical when integration tests
    /// re-register a domain with a fixture catalog).
    pub fn add_domain(&mut self, domain: Domain, catalogs: DomainRow) -> &mut Self {
        self.rows[domain as usize] = catalogs;
        self
    }

    /// Finalize the builder. The result is `Clone` and cheap to hand each
    /// plugin a copy of.
    #[must_use]
    pub fn build(self) -> Localizer {
        Localizer {
            inner: Arc::new(LocalizerInner {
                rows: self.rows,
                resolver: LocaleResolver::new(self.default_locale),
            }),
        }
    }
}

struct LocalizerInner {
    rows: [DomainRow; Domain::COUNT],
    resolver: LocaleResolver,
}

/// A shared catalog handle — one is built by the host at boot and cloned into
/// every plugin's `PluginResourceCtx`. Cheap to clone (an `Arc`).
#[derive(Clone)]
pub struct Localizer {
    inner: Arc<LocalizerInner>,
}

impl Localizer {
    /// The deployment-wide default this localizer was built with.
    #[must_use]
    pub fn default_locale(&self) -> Locale {
        self.inner.resolver.default_locale()
    }

    /// View the localizer through a specific [`Locale`] — the common entry
    /// point for jobs / `.ics` feeds where the locale is resolved up front.
    #[must_use]
    pub fn for_locale(&self, locale: Locale) -> ScopedLocalizer<'_> {
        ScopedLocalizer {
            inner: self.inner.as_ref(),
            locale,
        }
    }

    /// Resolve the locale for a stored preference string (e.g. the value of
    /// `platform.user.locale`), then return a scoped view.
    #[must_use]
    pub fn for_stored(&self, stored: Option<&str>) -> ScopedLocalizer<'_> {
        self.for_locale(self.inner.resolver.resolve_for_stored(stored))
    }

    /// Resolve the locale for an in-flight HTTP request: user preference →
    /// `Accept-Language` → deployment default.
    #[must_use]
    pub fn for_request(&self, user: Option<&User>, headers: &HeaderMap) -> ScopedLocalizer<'_> {
        self.for_locale(self.inner.resolver.resolve_for_request(user, headers))
    }

    /// Async helper that looks up `platform.user.locale` for the given user
    /// and returns a scoped view. Used by jobs that carry a user id but no
    /// pre-fetched preference.
    pub async fn for_user(
        &self,
        user_id: UserId,
        pool: &PgPool,
    ) -> Result<ScopedLocalizer<'_>, sqlx::Error> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT locale FROM platform.user WHERE id = $1")
                .bind(user_id.0)
                .fetch_optional(pool)
                .await?
                .flatten();
        Ok(self.for_stored(stored.as_deref()))
    }
}

/// A `Localizer` view at a fixed [`Locale`]. Built by [`Localizer::for_locale`]
/// and friends; calls into [`Self::t`] are O(1) array indexing.
#[derive(Copy, Clone)]
pub struct ScopedLocalizer<'a> {
    inner: &'a LocalizerInner,
    locale: Locale,
}

impl ScopedLocalizer<'_> {
    /// The locale this view is pinned to.
    #[must_use]
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// The deployment-wide default locale (the fallback the localizer was
    /// configured with — used when the active locale has no entry).
    #[must_use]
    pub fn default_locale(&self) -> Locale {
        self.inner.resolver.default_locale()
    }

    /// Translate `msg`. Looks up `M::DOMAIN × locale × M::ID`, falls back to
    /// the default locale's row, and finally to [`FALLBACK_TEMPLATE`] (which
    /// renders as the empty string — `junius i18n check` keeps this from
    /// happening in CI).
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Message values are tiny typed-field references with lifetimes; consuming by-value lets call sites write `t(Foo { name: ... })` without naming a local"
    )]
    pub fn t<M: Message>(&self, msg: M) -> String {
        let row = &self.inner.rows[M::DOMAIN as usize];
        let template = lookup(row, self.locale, M::ID)
            .or_else(|| lookup(row, self.inner.resolver.default_locale(), M::ID))
            .unwrap_or(&FALLBACK_TEMPLATE);
        msg.render(template)
    }
}

fn lookup(row: &DomainRow, locale: Locale, id: usize) -> Option<&'static Template> {
    row[locale as usize].get(id).copied().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::TemplatePart;

    // A hand-rolled "as if codegen'd" catalog — exercises the runtime
    // without depending on the build helper. The real codegen lives in
    // `junius-i18n-build` and is validated by an integration test there.
    struct Hello;
    impl Message for Hello {
        const DOMAIN: Domain = Domain::Hello;
        const ID: usize = 0;
        const KEY: &'static str = "hello.world";
        fn render(&self, template: &Template) -> String {
            template
                .parts
                .iter()
                .fold(String::new(), |mut acc, part| match *part {
                    TemplatePart::Literal(s) => {
                        acc.push_str(s);
                        acc
                    }
                    TemplatePart::Placeholder { .. } => acc,
                })
        }
    }

    struct Greeted<'a> {
        name: &'a str,
    }
    impl Message for Greeted<'_> {
        const DOMAIN: Domain = Domain::Hello;
        const ID: usize = 1;
        const KEY: &'static str = "hello.greeted";
        fn render(&self, template: &Template) -> String {
            let mut out = String::new();
            for part in template.parts {
                match *part {
                    TemplatePart::Literal(s) => out.push_str(s),
                    TemplatePart::Placeholder { name } => {
                        if name == "name" {
                            out.push_str(self.name);
                        } else {
                            out.push('{');
                            out.push_str(name);
                            out.push('}');
                        }
                    }
                }
            }
            out
        }
    }

    fn hello_catalogs() -> DomainRow {
        static EN: &[Option<&Template>] = &[
            Some(&Template {
                parts: &[TemplatePart::Literal("Hello, world")],
            }),
            Some(&Template {
                parts: &[
                    TemplatePart::Literal("Hi, "),
                    TemplatePart::Placeholder { name: "name" },
                ],
            }),
        ];
        static DE: &[Option<&Template>] = &[
            Some(&Template {
                parts: &[TemplatePart::Literal("Hallo, Welt")],
            }),
            // Greeted intentionally absent in `de` to exercise the fallback path.
            None,
        ];
        static PSEUDO: &[Option<&Template>] = &[
            Some(&Template {
                parts: &[TemplatePart::Literal("⟪Hello, world⟫")],
            }),
            Some(&Template {
                parts: &[
                    TemplatePart::Literal("⟪Hi, "),
                    TemplatePart::Placeholder { name: "name" },
                    TemplatePart::Literal("⟫"),
                ],
            }),
        ];
        [EN, DE, PSEUDO]
    }

    fn build() -> Localizer {
        let mut b = LocalizerBuilder::new(Locale::En);
        b.add_domain(Domain::Hello, hello_catalogs());
        b.build()
    }

    #[test]
    fn renders_active_locale() {
        let l = build();
        assert_eq!(l.for_locale(Locale::En).t(Hello), "Hello, world");
        assert_eq!(l.for_locale(Locale::De).t(Hello), "Hallo, Welt");
        assert_eq!(l.for_locale(Locale::Pseudo).t(Hello), "⟪Hello, world⟫");
    }

    #[test]
    fn substitutes_placeholders() {
        let l = build();
        assert_eq!(
            l.for_locale(Locale::En).t(Greeted { name: "Alice" }),
            "Hi, Alice"
        );
        assert_eq!(
            l.for_locale(Locale::Pseudo).t(Greeted { name: "Alice" }),
            "⟪Hi, Alice⟫"
        );
    }

    #[test]
    fn falls_back_to_default_locale_when_missing() {
        let l = build();
        // `Greeted` is absent in `de` → falls through to `en`.
        assert_eq!(
            l.for_locale(Locale::De).t(Greeted { name: "Bob" }),
            "Hi, Bob"
        );
    }

    #[test]
    fn empty_string_when_no_catalog_at_all() {
        // Domain row with no template for the message id → empty render.
        let mut b = LocalizerBuilder::new(Locale::En);
        // Hello slot stays at the EMPTY_ROW default → nothing registered.
        let _ = b.add_domain(Domain::Events, [&[], &[], &[]]);
        let l = b.build();
        assert_eq!(l.for_locale(Locale::En).t(Hello), "");
    }

    #[test]
    fn for_stored_resolves_through_resolver() {
        let l = build();
        assert_eq!(l.for_stored(Some("de")).locale(), Locale::De);
        assert_eq!(l.for_stored(Some("xx")).locale(), Locale::En);
        assert_eq!(l.for_stored(None).locale(), Locale::En);
    }
}
