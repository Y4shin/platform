//! M13 Stage 6 end-to-end: the `EventRepo` over a *least-privilege* `role_events`
//! pool, exercising the private/public × user/group access matrix through the
//! SECURITY DEFINER access functions. Covers: a private user event (owner sees,
//! stranger doesn't), publishing it (private→public makes it world-readable but
//! not editable), a private group event (members see + can edit, non-members
//! can't), and delete gated on write access. Skips cleanly without Docker.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::too_many_lines
)]

use chrono::{TimeZone, Utc};
use events_plugin::domain::{EventId, Visibility};
use events_plugin::repo::{EventRepo, EventUpdate, NewEvent};
use junius_sdk::{AuditEmitter, Authz, GroupId, PluginDb, Principal, RepoError, User, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

type ReadWrite = junius_sdk::permissions!(
    events_plugin::permissions::EventsRead & events_plugin::permissions::EventsWrite
);
type ReadOnly = junius_sdk::permissions!(events_plugin::permissions::EventsRead);

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
];
const EVENTS_MIGRATION: &str = include_str!("../../plugins/events/migrations/0001_event.up.sql");

// Mirrors the least-privilege set the migration runner produces for a plugin
// role: own-schema DML + USAGE on platform + SELECT on platform.user (EXECUTE on
// the SECURITY DEFINER access functions comes from the default PUBLIC grant).
const ROLE_EVENTS_GRANTS: &str = "\
    CREATE ROLE role_events LOGIN PASSWORD 'testpw' NOINHERIT; \
    GRANT USAGE, CREATE ON SCHEMA events TO role_events; \
    GRANT ALL ON ALL TABLES IN SCHEMA events TO role_events; \
    GRANT USAGE ON SCHEMA platform TO role_events; \
    GRANT SELECT ON platform.user TO role_events;";

async fn seed_user(pool: &PgPool, sub: &str) -> User {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.user (oidc_sub, email, display_name) \
         VALUES ($1, $1 || '@local', $1) RETURNING id",
    )
    .bind(sub)
    .fetch_one(pool)
    .await
    .unwrap();
    User {
        id: UserId(id),
        email: format!("{sub}@local"),
        display_name: sub.to_string(),
        memberships: vec![],
    }
}

/// Create a group with a "member" role granting `events:read`/`events:write`
/// (group-owned resources are reachable only via a role that grants the
/// permission — see `platform.user_can_access` step 2) and add `members` to it.
async fn seed_group(pool: &PgPool, name: &str, members: &[&User]) -> GroupId {
    let group_id: Uuid =
        sqlx::query_scalar("INSERT INTO platform.group (name) VALUES ($1) RETURNING id")
            .bind(name)
            .fetch_one(pool)
            .await
            .unwrap();
    let role_id: Uuid = sqlx::query_scalar(
        "INSERT INTO platform.group_role (group_id, name) VALUES ($1, 'member') RETURNING id",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await
    .unwrap();
    for perm in ["events:read", "events:write"] {
        sqlx::query("INSERT INTO platform.role_permission (role_id, permission) VALUES ($1, $2)")
            .bind(role_id)
            .bind(perm)
            .execute(pool)
            .await
            .unwrap();
    }
    for u in members {
        sqlx::query(
            "INSERT INTO platform.group_membership (user_id, group_id, role_id) \
             VALUES ($1, $2, $3)",
        )
        .bind(u.id.0)
        .bind(group_id)
        .bind(role_id)
        .execute(pool)
        .await
        .unwrap();
    }
    GroupId(group_id)
}

fn sample_event(title: &str, visibility: Visibility) -> NewEvent {
    NewEvent {
        title: title.to_string(),
        description: Some("desc".to_string()),
        location: Some("here".to_string()),
        starts_at: Utc.with_ymd_and_hms(2026, 6, 1, 18, 0, 0).unwrap(),
        ends_at: None,
        all_day: false,
        visibility,
    }
}

#[tokio::test]
async fn event_access_matrix_via_role_events() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping event_access_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS {
        sqlx::raw_sql(sql).execute(&admin).await.unwrap();
    }
    sqlx::raw_sql(EVENTS_MIGRATION)
        .execute(&admin)
        .await
        .unwrap();
    sqlx::raw_sql(ROLE_EVENTS_GRANTS)
        .execute(&admin)
        .await
        .unwrap();

    let alice = seed_user(&admin, "alice").await;
    let bob = seed_user(&admin, "bob").await;
    let carol = seed_user(&admin, "carol").await;
    // Alice + Carol are in the Committee; Bob is not.
    let committee = seed_group(&admin, "Committee", &[&alice, &carol]).await;

    // The plugin's least-privilege pool + the host's platform pool (for Authz).
    let role_pool = PgPool::connect(&format!(
        "postgres://role_events:testpw@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    let db = PluginDb::new(role_pool, "events");
    let audit = AuditEmitter::new(admin.clone());
    let authz = |u: &User| Authz::new(admin.clone()).with_user(Some(u.id));

    let alice_rw: EventRepo<ReadWrite> = EventRepo::new(&db, Some(alice.clone()), audit.clone());
    let alice_ro: EventRepo<ReadOnly> = EventRepo::new(&db, Some(alice.clone()), audit.clone());
    let bob_rw: EventRepo<ReadWrite> = EventRepo::new(&db, Some(bob.clone()), audit.clone());
    let bob_ro: EventRepo<ReadOnly> = EventRepo::new(&db, Some(bob.clone()), audit.clone());
    let carol_rw: EventRepo<ReadWrite> = EventRepo::new(&db, Some(carol.clone()), audit.clone());

    // --- private user event: owner sees + can edit; stranger sees nothing ----
    let priv_user = alice_rw
        .create(
            sample_event("Alice draft", Visibility::Private),
            Principal::User(alice.id),
            &authz(&alice),
        )
        .await
        .unwrap();
    assert!(priv_user.viewer_can_edit && priv_user.viewer_can_share);
    assert_eq!(alice_ro.list().await.unwrap().len(), 1);
    assert!(bob_ro.list().await.unwrap().is_empty());
    assert!(matches!(
        bob_ro.get(priv_user.id).await,
        Err(RepoError::NotFound)
    ));

    // --- publish (private→public): world-readable, but not editable by others -
    alice_rw
        .update(
            priv_user.id,
            EventUpdate {
                title: "Alice published".to_string(),
                description: priv_user.description.clone(),
                location: priv_user.location.clone(),
                starts_at: priv_user.starts_at,
                ends_at: None,
                all_day: false,
                visibility: Visibility::Public,
            },
        )
        .await
        .unwrap();
    let bob_view = bob_ro.get(priv_user.id).await.unwrap();
    assert_eq!(bob_view.title, "Alice published");
    assert!(!bob_view.viewer_can_edit && !bob_view.viewer_can_share);
    // Bob can't edit or delete a public event he doesn't own.
    assert!(matches!(
        bob_rw.update(priv_user.id, edit_of(&bob_view)).await,
        Err(RepoError::NotFound)
    ));
    assert!(matches!(
        bob_rw.delete(priv_user.id).await,
        Err(RepoError::NotFound)
    ));

    // --- private group event: members see + can edit; non-members can't -------
    let priv_group = alice_rw
        .create(
            sample_event("Committee meeting", Visibility::Private),
            Principal::Group(committee),
            &authz(&alice),
        )
        .await
        .unwrap();
    // Carol (member) sees it and can edit; Bob (non-member) cannot see it.
    let carol_list = carol_rw.list().await.unwrap();
    assert!(carol_list.iter().any(|e| e.id == priv_group.id));
    let carol_view = carol_rw.get(priv_group.id).await.unwrap();
    assert!(carol_view.viewer_can_edit);
    assert!(matches!(
        bob_ro.get(priv_group.id).await,
        Err(RepoError::NotFound)
    ));
    // A member (Carol) can edit the group event…
    carol_rw
        .update(priv_group.id, edit_of(&carol_view))
        .await
        .unwrap();
    // …a non-member (Bob) cannot.
    assert!(matches!(
        bob_rw.update(priv_group.id, edit_of(&carol_view)).await,
        Err(RepoError::NotFound)
    ));

    // --- delete gated on write access ----------------------------------------
    alice_rw.delete(priv_group.id).await.unwrap();
    assert!(matches!(
        alice_ro.get(priv_group.id).await,
        Err(RepoError::NotFound)
    ));

    // The create/update/delete were audited.
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT event_kind FROM platform.audit_event ORDER BY 1")
            .fetch_all(&admin)
            .await
            .unwrap();
    assert!(kinds.contains(&"events:event.create".to_string()));
    assert!(kinds.contains(&"events:event.update".to_string()));
    assert!(kinds.contains(&"events:event.delete".to_string()));

    // Belt-and-braces: a bogus id is NotFound, not a panic.
    assert!(matches!(
        alice_ro.get(EventId(Uuid::nil())).await,
        Err(RepoError::NotFound)
    ));
}

/// Build an `EventUpdate` that re-saves a view unchanged (for "can this caller
/// even edit?" probes).
fn edit_of(v: &events_plugin::repo::EventView) -> EventUpdate {
    EventUpdate {
        title: v.title.clone(),
        description: v.description.clone(),
        location: v.location.clone(),
        starts_at: v.starts_at,
        ends_at: v.ends_at,
        all_day: v.all_day,
        visibility: v.visibility,
    }
}
