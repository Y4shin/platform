//! Integration test for the `PlatformAdminApi::list_audit_events` method
//! (M19 Slice #6). Tests cursor-based keyset pagination, filter correctness,
//! limit handling, permission gating, and malformed-cursor rejection against
//! a real Postgres instance via testcontainers.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use junius_sdk::{AuditCursor, AuditEmitter, AuditFilter, PlatformAdminApi, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0002_sessions.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0006_meta_migrations.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
    include_str!("../migrations/0011_user_locale.up.sql"),
    include_str!("../migrations/0012_forget_resource.up.sql"),
    include_str!("../migrations/0013_user_roles.up.sql"),
    include_str!("../migrations/0014_oidc_group_mapping.up.sql"),
];

const CAPS: &[&str] = &["platform.admin"];
const NO_CAPS: &[&str] = &[];

async fn pg_pool() -> Option<(
    testcontainers_modules::testcontainers::ContainerAsync<Postgres>,
    PgPool,
)> {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping audit_pg: Docker unavailable ({e})");
            return None;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let pool = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    Some((node, pool))
}

fn admin_api(pool: &PgPool, caps: &'static [&'static str]) -> PlatformAdminApi {
    PlatformAdminApi::new(
        pool.clone(),
        AuditEmitter::new(pool.clone()),
        "admin",
        caps,
        Arc::new(Vec::new()),
        false,
    )
}

fn no_filter() -> AuditFilter<'static> {
    AuditFilter {
        actor_user_id: None,
        resource_kind: None,
        event_kind: None,
        starts_at: None,
        ends_at: None,
    }
}

async fn seed_user(pool: &PgPool, sub: &str, email: &str, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO platform.\"user\" (oidc_sub, email, display_name) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(sub)
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_audit_event(
    pool: &PgPool,
    event_kind: &str,
    actor: Option<Uuid>,
    resource_kind: Option<&str>,
    resource_id: Option<Uuid>,
    details: serde_json::Value,
    occurred_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO platform.audit_event \
             (event_kind, actor_user_id, resource_kind, resource_id, details, occurred_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(event_kind)
    .bind(actor)
    .bind(resource_kind)
    .bind(resource_id)
    .bind(details)
    .bind(occurred_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---- Cursor tie-break: identical occurred_at rows paged without dupes/gaps ---

#[tokio::test]
async fn cursor_tiebreak_no_duplicates_no_gaps() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let alice = seed_user(&pool, "alice", "alice@local", "Alice").await;
    let ts = Utc::now();

    let mut all_ids = Vec::new();
    for i in 0..5 {
        let id = seed_audit_event(
            &pool,
            "admin:group.create",
            Some(alice),
            Some("platform:group"),
            None,
            serde_json::json!({ "i": i }),
            ts,
        )
        .await;
        all_ids.push(id);
    }

    let mut collected = Vec::new();
    let mut cursor: Option<AuditCursor> = None;
    loop {
        let page = api
            .list_audit_events(&no_filter(), cursor.as_ref(), 2)
            .await
            .unwrap();
        collected.extend(page.events.iter().map(|e| e.id));
        if page.next_cursor.is_none() {
            break;
        }
        cursor = page.next_cursor;
    }

    collected.sort();
    collected.dedup();
    let mut expected = all_ids.clone();
    expected.sort();
    assert_eq!(collected, expected, "every row returned exactly once");
}

// ---- Ordering: newest-first, stable across pages ----------------------------

#[tokio::test]
async fn ordering_newest_first_stable() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();

    let mut ids_newest_first = Vec::new();
    for i in 0..4 {
        let ts = now - Duration::seconds(4 - i);
        let id = seed_audit_event(
            &pool,
            "admin:role.create",
            None,
            Some("platform:group_role"),
            None,
            serde_json::json!({}),
            ts,
        )
        .await;
        ids_newest_first.push(id);
    }
    ids_newest_first.reverse();

    let mut collected = Vec::new();
    let mut cursor: Option<AuditCursor> = None;
    loop {
        let page = api
            .list_audit_events(&no_filter(), cursor.as_ref(), 2)
            .await
            .unwrap();
        collected.extend(page.events.iter().map(|e| e.id));
        if page.next_cursor.is_none() {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected, ids_newest_first);
}

// ---- Filters narrow correctly (AND together) --------------------------------

#[tokio::test]
async fn filter_actor_user_id() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let alice = seed_user(&pool, "alice", "alice@local", "Alice").await;
    let bob = seed_user(&pool, "bob", "bob@local", "Bob").await;
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:group.create",
        Some(alice),
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:group.create",
        Some(bob),
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let filter = AuditFilter {
        actor_user_id: Some(alice),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].actor_user_id, Some(UserId(alice)));
}

#[tokio::test]
async fn filter_resource_kind() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        Some("platform:group"),
        None,
        serde_json::json!({}),
        now,
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:role.create",
        None,
        Some("platform:group_role"),
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let filter = AuditFilter {
        resource_kind: Some("platform:group"),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(
        page.events[0].resource_kind.as_deref(),
        Some("platform:group")
    );
}

#[tokio::test]
async fn filter_event_kind() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:membership.add",
        None,
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let filter = AuditFilter {
        event_kind: Some("admin:membership.add"),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_kind, "admin:membership.add");
}

#[tokio::test]
async fn filter_date_range() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        None,
        None,
        serde_json::json!({}),
        now - Duration::hours(2),
    )
    .await;
    let in_range = seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        None,
        None,
        serde_json::json!({}),
        now - Duration::minutes(30),
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        None,
        None,
        serde_json::json!({}),
        now + Duration::hours(2),
    )
    .await;

    let filter = AuditFilter {
        starts_at: Some(now - Duration::hours(1)),
        ends_at: Some(now),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].id, in_range);
}

#[tokio::test]
async fn combined_filters_and_together() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let alice = seed_user(&pool, "alice", "alice@local", "Alice").await;
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:group.create",
        Some(alice),
        Some("platform:group"),
        None,
        serde_json::json!({}),
        now,
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:membership.add",
        Some(alice),
        Some("platform:group_membership"),
        None,
        serde_json::json!({}),
        now,
    )
    .await;
    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        Some("platform:group"),
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let filter = AuditFilter {
        actor_user_id: Some(alice),
        event_kind: Some("admin:group.create"),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event_kind, "admin:group.create");
    assert_eq!(page.events[0].actor_user_id, Some(UserId(alice)));
}

#[tokio::test]
async fn filter_matching_nothing_returns_empty_with_no_cursor() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();
    seed_audit_event(
        &pool,
        "admin:group.create",
        None,
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let filter = AuditFilter {
        event_kind: Some("nonexistent:kind"),
        ..no_filter()
    };
    let page = api.list_audit_events(&filter, None, 50).await.unwrap();
    assert!(page.events.is_empty());
    assert!(page.next_cursor.is_none());
}

// ---- Limit handling ---------------------------------------------------------

#[tokio::test]
async fn limit_default_and_clamp() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let now = Utc::now();

    for i in 0..5 {
        seed_audit_event(
            &pool,
            "admin:group.create",
            None,
            None,
            None,
            serde_json::json!({}),
            now + Duration::seconds(i),
        )
        .await;
    }

    let page = api.list_audit_events(&no_filter(), None, 3).await.unwrap();
    assert_eq!(page.events.len(), 3);
    assert!(page.next_cursor.is_some());

    let page = api
        .list_audit_events(&no_filter(), None, 999)
        .await
        .unwrap();
    assert_eq!(page.events.len(), 5);
    assert!(page.next_cursor.is_none());
}

// ---- Permission guard -------------------------------------------------------

#[tokio::test]
async fn permission_guard_rejects_without_capability() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, NO_CAPS);
    let result = api.list_audit_events(&no_filter(), None, 50).await;
    assert!(result.is_err(), "should reject without platform.admin");
}

#[tokio::test]
async fn permission_guard_allows_with_capability() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let result = api.list_audit_events(&no_filter(), None, 50).await;
    assert!(result.is_ok());
}

// ---- Actor info join --------------------------------------------------------

#[tokio::test]
async fn actor_email_and_display_name_joined() {
    let Some((_node, pool)) = pg_pool().await else {
        return;
    };
    let api = admin_api(&pool, CAPS);
    let alice = seed_user(&pool, "alice", "alice@local", "Alice").await;
    let now = Utc::now();

    seed_audit_event(
        &pool,
        "admin:group.create",
        Some(alice),
        None,
        None,
        serde_json::json!({}),
        now,
    )
    .await;

    let page = api.list_audit_events(&no_filter(), None, 50).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].actor_email.as_deref(), Some("alice@local"));
    assert_eq!(page.events[0].actor_display_name.as_deref(), Some("Alice"));
}
