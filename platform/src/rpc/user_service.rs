//! `user.v1.UserService` — the host's own Connect-RPC service (M18).
//!
//! Two methods, both reconciling a user's `managed_by='oidc'` memberships via
//! the shared [`oidc_groups::fetch_oidc_groups`] + [`oidc_groups::reconcile_memberships`]
//! path:
//!
//! * [`refresh_oidc_groups`](UserRpc::refresh_oidc_groups) — the authenticated
//!   caller's self-refresh (the typed twin of `POST /api/me/refresh-groups`).
//! * [`resync_oidc_groups`](UserRpc::resync_oidc_groups) — an ops sweep over
//!   one or all users, gated by the deployment's `admin_api_token` (surfaced as
//!   the [`junius_sdk::AdminAuth`] marker by the `/rpc` admin-token middleware).
//!
//! The service is self-contained: it carries its own pool/cipher/userinfo URL,
//! so the host RPC stack needs no `PluginResourceCtx` injection for it. Auth is
//! enforced in-handler via [`junius_sdk::HostCtx`].

use aes_gcm::Aes256Gcm;
use connectrpc::{Encodable, Response, ServiceResult};
use junius_sdk::{ApiError, HostCtx};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{AuthState, oidc_groups};

use super::proto::user::v1 as pb;
use super::proto::user::v1::{
    OwnedRefreshOidcGroupsRequestView, OwnedResyncOidcGroupsRequestView, UserService,
};

/// Host `user.v1.UserService` implementation. Holds the slice of [`AuthState`]
/// it needs so the RPC stack doesn't have to inject any per-request context.
pub struct UserRpc {
    pool: PgPool,
    cipher: Aes256Gcm,
    userinfo_url: Option<String>,
}

impl UserRpc {
    /// Build from the host's [`AuthState`].
    #[must_use]
    pub fn from_auth_state(state: &AuthState) -> Self {
        Self {
            pool: state.pool.clone(),
            cipher: state.cipher.clone(),
            userinfo_url: state.oidc_userinfo_url.clone(),
        }
    }

    /// The configured userinfo endpoint, or a 500 when OIDC discovery failed
    /// at boot (login is then disabled anyway).
    fn userinfo_url(&self) -> Result<&str, ApiError> {
        self.userinfo_url
            .as_deref()
            .ok_or_else(|| ApiError::internal("OIDC userinfo endpoint not configured"))
    }

    /// Resolve the set of `(user_id, email)` an admin sweep should reconcile.
    /// `all` ⇒ every user with a live session carrying a stored token; `user`
    /// ⇒ a single user matched by UUID, email, or OIDC subject.
    async fn resolve_targets(
        &self,
        user: Option<&str>,
        all: bool,
    ) -> Result<Vec<(Uuid, String)>, ApiError> {
        match (user, all) {
            (Some(ident), _) => {
                let row: Option<(Uuid, String)> = if let Ok(id) = Uuid::parse_str(ident) {
                    sqlx::query_as("SELECT id, email FROM platform.user WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&self.pool)
                        .await
                } else {
                    sqlx::query_as(
                        "SELECT id, email FROM platform.user \
                         WHERE email = $1 OR oidc_sub = $1",
                    )
                    .bind(ident)
                    .fetch_optional(&self.pool)
                    .await
                }
                .map_err(|e| ApiError::internal(format!("user lookup failed: {e}")))?;
                row.map(|r| vec![r])
                    .ok_or_else(|| ApiError::internal(format!("user not found: {ident}")))
            }
            (None, true) => sqlx::query_as(
                "SELECT DISTINCT u.id, u.email \
                 FROM platform.user u \
                 JOIN platform.session s ON s.user_id = u.id \
                 WHERE s.expires_at > now() AND s.oidc_tokens IS NOT NULL \
                 ORDER BY u.email",
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ApiError::internal(format!("user sweep query failed: {e}"))),
            (None, false) => Err(ApiError::internal("specify a target: set `user` or `all`")),
        }
    }
}

#[junius_sdk::rpc_service(UserService)]
impl UserRpc {
    /// Re-pull the authenticated caller's OIDC `groups` claim and reconcile.
    async fn refresh_oidc_groups(
        &self,
        ctx: HostCtx<crate::rpc::__rpc_requires::user_service::RefreshOidcGroups>,
        _request: OwnedRefreshOidcGroupsRequestView,
    ) -> ServiceResult<impl Encodable<pb::ReconcileResult>> {
        let user = ctx.user.ok_or_else(ApiError::unauthenticated)?;
        let userinfo_url = self.userinfo_url()?;
        let groups =
            oidc_groups::fetch_oidc_groups(&self.pool, &self.cipher, userinfo_url, user.id.0)
                .await
                .map_err(map_self_fetch_err)?;
        let summary = oidc_groups::reconcile_memberships(&self.pool, user.id.0, &groups)
            .await
            .map_err(|e| ApiError::internal(format!("reconcile failed: {e}")))?;
        Ok(Response::new(summary_to_proto(&summary)))
    }

    /// Ops sweep: reconcile one user or all users with a live token. Gated by
    /// the admin API token. Never fails the whole sweep on a per-user problem —
    /// the user is reported as `skipped` with a reason instead.
    async fn resync_oidc_groups(
        &self,
        ctx: HostCtx<crate::rpc::__rpc_requires::user_service::ResyncOidcGroups>,
        request: OwnedResyncOidcGroupsRequestView,
    ) -> ServiceResult<impl Encodable<pb::ResyncOidcGroupsResponse>> {
        if !ctx.is_admin_token() {
            return Err(ApiError::unauthenticated().into());
        }
        let targets = self.resolve_targets(request.user, request.all).await?;
        if targets.is_empty() {
            return Ok(Response::new(pb::ResyncOidcGroupsResponse::default()));
        }
        let userinfo_url = self.userinfo_url()?;

        let mut results = Vec::with_capacity(targets.len());
        for (user_id, email) in targets {
            let entry = match oidc_groups::fetch_oidc_groups(
                &self.pool,
                &self.cipher,
                userinfo_url,
                user_id,
            )
            .await
            {
                Ok(groups) => {
                    match oidc_groups::reconcile_memberships(&self.pool, user_id, &groups).await {
                        Ok(summary) => pb::UserReconcileResult {
                            user_id: user_id.to_string(),
                            email,
                            result: Some(summary_to_proto(&summary)).into(),
                            skipped: None,
                            ..Default::default()
                        },
                        Err(e) => skipped(user_id, email, format!("reconcile failed: {e}")),
                    }
                }
                Err(e) => skipped(user_id, email, e.to_string()),
            };
            results.push(entry);
        }
        Ok(Response::new(pb::ResyncOidcGroupsResponse {
            results,
            ..Default::default()
        }))
    }
}

/// Map a [`oidc_groups::FetchGroupsError`] for the self-refresh path: the
/// session-relative failures are 401s (re-login), the rest are 500s.
fn map_self_fetch_err(e: oidc_groups::FetchGroupsError) -> ApiError {
    use oidc_groups::FetchGroupsError as E;
    match e {
        E::NoActiveSession | E::NoStoredToken | E::TokenExpired => ApiError::unauthenticated(),
        other => ApiError::internal(other.to_string()),
    }
}

/// One reconcile summary → its proto counterpart.
fn summary_to_proto(s: &oidc_groups::ReconcileSummary) -> pb::ReconcileResult {
    pb::ReconcileResult {
        oidc_groups_claimed: u32::try_from(s.oidc_groups_claimed).unwrap_or(u32::MAX),
        memberships_added: u32::try_from(s.mappings_applied).unwrap_or(u32::MAX),
        memberships_reaped: u32::try_from(s.memberships_reaped).unwrap_or(u32::MAX),
        ..Default::default()
    }
}

/// A skipped per-user result with a human-readable reason.
fn skipped(user_id: Uuid, email: String, reason: String) -> pb::UserReconcileResult {
    pb::UserReconcileResult {
        user_id: user_id.to_string(),
        email,
        result: None.into(),
        skipped: Some(reason),
        ..Default::default()
    }
}
