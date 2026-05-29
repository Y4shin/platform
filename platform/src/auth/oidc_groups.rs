//! M18 Stage C — OIDC group → Junius group/role reconciliation.
//!
//! Triggered from four call sites that all hand the same `(user_id,
//! oidc_groups)` pair to [`reconcile_memberships`]:
//!
//! 1. The OIDC callback at login (every successful auth flow).
//! 2. `POST /api/me/refresh-groups` — the authenticated user's REST hook.
//! 3. `UserService.RefreshOidcGroups` — the Connect-RPC counterpart.
//! 4. `junius oidc resync` CLI — ops sweep without a session.
//!
//! Reconciliation is provenance-aware: `managed_by='oidc'` memberships are
//! the OIDC reconciler's own rows, freely added / reaped; `'manual'` and
//! `'config'` rows are left strictly alone. The single guard against drift
//! is that a UNIQUE `(user_id, group_id)` constraint means at most one
//! membership per group — the upsert that turns a `manual` row into `oidc`
//! is explicitly forbidden by the `WHERE managed_by='oidc'` clause in the
//! ON CONFLICT.

use aes_gcm::Aes256Gcm;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::crypto;

/// Default claim key per the M18 spec; configurable per-deployment when the
/// [`junius_manifest::ResolvedConfig`] grows an `oidc_groups_claim` knob.
/// Authentik's default property mapping for groups uses this key.
pub const DEFAULT_GROUPS_CLAIM: &str = "groups";

/// Decode the *middle* segment of an OIDC id-token JWT and pull `claim_key`
/// out of the JSON payload, treating it as a `Vec<String>`. Returns an empty
/// vector if the claim is missing, malformed, or the JWT itself is not
/// three base64url-segments separated by dots.
///
/// We deliberately do **not** verify the signature here — the caller already
/// verified the token via `openidconnect`'s `id_token.claims(..., nonce)`
/// before calling this. This is a pure-syntax accessor for one claim that
/// the typed `CoreIdTokenClaims` doesn't expose.
#[must_use]
pub fn extract_groups_from_id_token(id_token: &str, claim_key: &str) -> Vec<String> {
    let mut parts = id_token.split('.');
    let (_, payload, _) = (parts.next(), parts.next(), parts.next());
    let Some(payload_b64) = payload else {
        return Vec::new();
    };
    let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload_b64) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&payload_bytes) else {
        return Vec::new();
    };
    json.get(claim_key)
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One mapping row, materialised for use by the reconciler.
#[derive(Debug, Clone, Deserialize)]
struct Mapping {
    group_id: Uuid,
    role_id: Uuid,
    oidc_group_name: String,
}

/// Reconcile a user's `managed_by='oidc'` group memberships against the
/// `oidc_groups` claim. Idempotent. Manual / config memberships are
/// untouched: the upsert's `ON CONFLICT DO UPDATE` clause refuses to
/// promote them, and the reaper's `WHERE managed_by='oidc'` clause refuses
/// to delete them.
pub async fn reconcile_memberships(
    pool: &PgPool,
    user_id: Uuid,
    oidc_groups: &[String],
) -> Result<ReconcileSummary, sqlx::Error> {
    if oidc_groups.is_empty() {
        // No groups claim → only reap stale OIDC memberships.
        return reap_stale(pool, user_id, &[]).await;
    }

    let mappings: Vec<Mapping> = sqlx::query_as::<_, (Uuid, Uuid, String)>(
        "SELECT group_id, role_id, oidc_group_name \
         FROM platform.oidc_group_mapping \
         WHERE oidc_group_name = ANY($1)",
    )
    .bind(oidc_groups)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(group_id, role_id, oidc_group_name)| Mapping {
        group_id,
        role_id,
        oidc_group_name,
    })
    .collect();

    let mut added = 0usize;
    for m in &mappings {
        // Upsert: if a row already exists and is managed_by='oidc', refresh
        // role_id + managed_source. If it's 'manual'/'config', leave it
        // alone — the WHERE clause on the DO UPDATE ensures that.
        let res = sqlx::query(
            "INSERT INTO platform.group_membership \
                 (user_id, group_id, role_id, managed_by, managed_source) \
             VALUES ($1, $2, $3, 'oidc', $4) \
             ON CONFLICT (user_id, group_id) DO UPDATE SET \
                 role_id        = EXCLUDED.role_id, \
                 managed_source = EXCLUDED.managed_source \
             WHERE platform.group_membership.managed_by = 'oidc'",
        )
        .bind(user_id)
        .bind(m.group_id)
        .bind(m.role_id)
        .bind(&m.oidc_group_name)
        .execute(pool)
        .await?;
        added += usize::try_from(res.rows_affected()).unwrap_or(0);
    }

    let active_sources: Vec<String> = mappings.iter().map(|m| m.oidc_group_name.clone()).collect();
    let reaped = reap_stale(pool, user_id, &active_sources).await?;

    Ok(ReconcileSummary {
        oidc_groups_claimed: oidc_groups.len(),
        mappings_applied: added,
        memberships_reaped: reaped.memberships_reaped,
    })
}

/// Delete `managed_by='oidc'` memberships whose `managed_source` is not in
/// the user's current OIDC-groups claim. Always runs after the upsert pass.
async fn reap_stale(
    pool: &PgPool,
    user_id: Uuid,
    active_sources: &[String],
) -> Result<ReconcileSummary, sqlx::Error> {
    let res = sqlx::query(
        "DELETE FROM platform.group_membership \
         WHERE user_id    = $1 \
           AND managed_by = 'oidc' \
           AND NOT (managed_source = ANY($2))",
    )
    .bind(user_id)
    .bind(active_sources)
    .execute(pool)
    .await?;
    let reaped = usize::try_from(res.rows_affected()).unwrap_or(0);
    Ok(ReconcileSummary {
        oidc_groups_claimed: active_sources.len(),
        mappings_applied: 0,
        memberships_reaped: reaped,
    })
}

/// Diagnostic counters returned by [`reconcile_memberships`]; useful for the
/// `junius oidc resync` CLI's report and the response body of the on-demand
/// REST/RPC endpoints (so a webhook caller can verify their trigger
/// landed).
#[derive(Debug, Clone, Default)]
pub struct ReconcileSummary {
    pub oidc_groups_claimed: usize,
    pub mappings_applied: usize,
    pub memberships_reaped: usize,
}

/// Failure modes of [`fetch_oidc_groups`], kept distinct so callers can map to
/// a precise HTTP status (the REST self-refresh handler) or a per-user skip
/// reason (the admin sweep), without re-deriving the cause from a string.
#[derive(Debug)]
pub enum FetchGroupsError {
    /// The user has no live (unexpired) session row to borrow a token from.
    NoActiveSession,
    /// The latest live session carries no stored OIDC tokens.
    NoStoredToken,
    /// Decrypting the stored access token failed.
    TokenDecrypt,
    /// The `IdP` rejected the access token (expired) — the user must re-login.
    TokenExpired,
    /// The `userinfo` request failed, returned non-success, or wasn't JSON.
    Upstream(String),
    /// Database error during the session lookup.
    Db(sqlx::Error),
}

impl std::fmt::Display for FetchGroupsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActiveSession => write!(f, "no active session"),
            Self::NoStoredToken => write!(f, "no stored OIDC tokens on this session"),
            Self::TokenDecrypt => write!(f, "token decryption failed"),
            Self::TokenExpired => write!(f, "OIDC access token expired"),
            Self::Upstream(m) => write!(f, "userinfo upstream error: {m}"),
            Self::Db(e) => write!(f, "session lookup failed: {e}"),
        }
    }
}

impl std::error::Error for FetchGroupsError {}

/// Pull a user's current OIDC `groups` claim by calling the `IdP` `userinfo`
/// endpoint with the access token stored on their most recent live session.
///
/// Shared by every off-login trigger: the REST `/api/me/refresh-groups`
/// handler, and the host `UserService` RPC's self-refresh + admin-sweep
/// methods. Raw `reqwest` (not `openidconnect`'s typed `user_info`) because
/// the typed API doesn't surface the `groups` claim without changing the
/// `AdditionalClaims` type parameter across the whole auth module; the access
/// token is already trusted and JSON extraction is a one-liner.
pub async fn fetch_oidc_groups(
    pool: &PgPool,
    cipher: &Aes256Gcm,
    userinfo_url: &str,
    user_id: Uuid,
) -> Result<Vec<String>, FetchGroupsError> {
    let row = sqlx::query_as::<_, (Option<Vec<u8>>,)>(
        "SELECT oidc_tokens FROM platform.session \
         WHERE user_id = $1 AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(FetchGroupsError::Db)?;

    let Some((token_blob,)) = row else {
        return Err(FetchGroupsError::NoActiveSession);
    };
    let Some(token_blob) = token_blob else {
        return Err(FetchGroupsError::NoStoredToken);
    };
    let Some(access_bytes) = crypto::decrypt(cipher, &token_blob) else {
        return Err(FetchGroupsError::TokenDecrypt);
    };
    let access_token = String::from_utf8_lossy(&access_bytes).into_owned();

    let client = reqwest::Client::new();
    let resp = client
        .get(userinfo_url)
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|e| FetchGroupsError::Upstream(e.to_string()))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(FetchGroupsError::TokenExpired);
    }
    if !resp.status().is_success() {
        return Err(FetchGroupsError::Upstream(format!(
            "userinfo returned {}",
            resp.status()
        )));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FetchGroupsError::Upstream(format!("userinfo response not JSON: {e}")))?;
    Ok(body
        .get(DEFAULT_GROUPS_CLAIM)
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}
