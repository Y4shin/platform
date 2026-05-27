//! M13 Stage 9 end-to-end: the calendar feed queries + token lifecycle over a
//! least-privilege `role_events` pool. Covers the single-event ACL, the personal
//! feed (owned/group + signed-up), the group feed, token mint/lookup/revoke, and
//! the per-group public toggle. Skips cleanly without Docker.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::too_many_lines
)]

use chrono::{TimeZone, Utc};
use events_plugin::domain::{FeedKind, Visibility, generate_feed_key, hash_token};
use events_plugin::repo::{CalendarRepo, EventRepo, NewEvent};
use junius_sdk::{AuditEmitter, Authz, GroupId, PluginDb, Principal, RepoError, User, UserId};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

type ReadWrite = junius_sdk::permissions!(
    events_plugin::permissions::EventsRead & events_plugin::permissions::EventsWrite
);

const HOST_MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_users.up.sql"),
    include_str!("../migrations/0003_groups_roles_memberships.up.sql"),
    include_str!("../migrations/0004_resource_principal_share.up.sql"),
    include_str!("../migrations/0005_user_can_access.up.sql"),
    include_str!("../migrations/0007_audit_event.up.sql"),
    include_str!("../migrations/0008_authz_functions.up.sql"),
];
const EVENTS_MIGRATIONS: &[&str] = &[
    include_str!("../../plugins/events/migrations/0001_event.up.sql"),
    include_str!("../../plugins/events/migrations/0002_invite.up.sql"),
    include_str!("../../plugins/events/migrations/0003_signup.up.sql"),
    include_str!("../../plugins/events/migrations/0004_indexes.up.sql"),
    include_str!("../../plugins/events/migrations/0005_calendar_token.up.sql"),
];

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
        locale: None,
        memberships: vec![],
        user_roles: vec![],
    }
}

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

fn sample(title: &str, vis: Visibility) -> NewEvent {
    NewEvent {
        title: title.to_string(),
        description: None,
        location: None,
        starts_at: Utc.with_ymd_and_hms(2026, 6, 1, 18, 0, 0).unwrap(),
        ends_at: None,
        all_day: false,
        visibility: vis,
    }
}

#[tokio::test]
async fn calendar_feeds_and_tokens_via_role_events() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping calendar_pg: Docker unavailable ({e})");
            return;
        }
    };
    let port = node.get_host_port_ipv4(5432).await.unwrap();
    let admin = PgPool::connect(&format!(
        "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    for sql in HOST_MIGRATIONS.iter().chain(EVENTS_MIGRATIONS) {
        sqlx::raw_sql(sql).execute(&admin).await.unwrap();
    }
    sqlx::raw_sql(ROLE_EVENTS_GRANTS)
        .execute(&admin)
        .await
        .unwrap();

    let alice = seed_user(&admin, "alice").await;
    let bob = seed_user(&admin, "bob").await;
    let committee = seed_group(&admin, "Committee", &[&alice]).await;

    let role_pool = PgPool::connect(&format!(
        "postgres://role_events:testpw@127.0.0.1:{port}/postgres"
    ))
    .await
    .unwrap();
    let db = PluginDb::new(role_pool, "events");
    let audit = AuditEmitter::new(admin.clone());
    let authz = Authz::new(admin.clone()).with_user(Some(alice.id));

    let alice_events: EventRepo<ReadWrite> =
        EventRepo::new(&db, Some(alice.clone()), audit.clone());
    let public = alice_events
        .create(
            sample("Public", Visibility::Public),
            Principal::User(alice.id),
            &authz,
        )
        .await
        .unwrap();
    let private = alice_events
        .create(
            sample("Private", Visibility::Private),
            Principal::User(alice.id),
            &authz,
        )
        .await
        .unwrap();
    let group_event = alice_events
        .create(
            sample("Group AGM", Visibility::Private),
            Principal::Group(committee),
            &authz,
        )
        .await
        .unwrap();

    // Caller-less calendar repo (the .ics handlers' shape) + a gated one for mint.
    let cal: CalendarRepo<()> = CalendarRepo::new(&db, None, audit.clone());
    let cal_rw: CalendarRepo<ReadWrite> =
        CalendarRepo::new(&db, Some(alice.clone()), audit.clone());

    // --- single-event ACL ----------------------------------------------------
    assert!(
        cal.event_for_ics(public.id.0, None)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        cal.event_for_ics(private.id.0, None)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        cal.event_for_ics(private.id.0, Some(alice.id.0))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        cal.event_for_ics(private.id.0, Some(bob.id.0))
            .await
            .unwrap()
            .is_none()
    );

    // --- personal feed -------------------------------------------------------
    // Alice: all three of her events (owned + group member).
    let alice_feed = cal.personal_feed(alice.id.0).await.unwrap();
    assert_eq!(alice_feed.len(), 3);
    // Bob: nothing yet.
    assert!(cal.personal_feed(bob.id.0).await.unwrap().is_empty());
    // Bob signs up to the public event → it appears in his feed (live).
    let invite_id: Uuid = sqlx::query_scalar(
        "INSERT INTO events.invite (event_id, slug) VALUES ($1, 'cal-slug') RETURNING id",
    )
    .bind(public.id.0)
    .fetch_one(&admin)
    .await
    .unwrap();
    sqlx::query("INSERT INTO events.signup (invite_id, kind, user_id) VALUES ($1, 'user', $2)")
        .bind(invite_id)
        .bind(bob.id.0)
        .execute(&admin)
        .await
        .unwrap();
    let bob_feed = cal.personal_feed(bob.id.0).await.unwrap();
    assert_eq!(bob_feed.len(), 1);
    assert_eq!(bob_feed[0].id, public.id.0);

    // --- group feed ----------------------------------------------------------
    let group_feed = cal.group_feed(committee.0).await.unwrap();
    assert_eq!(group_feed.len(), 1);
    assert_eq!(group_feed[0].id, group_event.id.0);

    // --- token mint / lookup / revoke ---------------------------------------
    let key = generate_feed_key();
    let token_id = cal_rw
        .mint_token(
            FeedKind::Personal,
            Some(alice.id.0),
            None,
            alice.id.0,
            Some("My phone"),
            &hash_token(&key),
        )
        .await
        .unwrap();
    let resolved = cal.lookup_token(&hash_token(&key)).await.unwrap().unwrap();
    assert_eq!(resolved.kind, FeedKind::Personal);
    assert_eq!(resolved.subject_user_id, Some(alice.id.0));
    // A bogus key resolves to nothing.
    assert!(
        cal.lookup_token(&hash_token("nope"))
            .await
            .unwrap()
            .is_none()
    );
    // Bob can't revoke Alice's token; Alice can. After revoke it stops serving.
    assert!(matches!(
        cal_rw_for(&db, &bob, &audit)
            .revoke_token(token_id, bob.id.0)
            .await,
        Err(RepoError::NotFound)
    ));
    cal_rw.revoke_token(token_id, alice.id.0).await.unwrap();
    assert!(cal.lookup_token(&hash_token(&key)).await.unwrap().is_none());

    // list_tokens shows Alice's (now-revoked) token.
    let listed = cal_rw.list_tokens(alice.id.0).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].revoked);

    // --- group public toggle -------------------------------------------------
    assert!(!cal.group_is_public(committee.0).await.unwrap());
    cal_rw
        .set_group_public(committee.0, true, alice.id.0)
        .await
        .unwrap();
    assert!(cal.group_is_public(committee.0).await.unwrap());
    // Idempotent upsert: flip back off.
    cal_rw
        .set_group_public(committee.0, false, alice.id.0)
        .await
        .unwrap();
    assert!(!cal.group_is_public(committee.0).await.unwrap());
}

fn cal_rw_for(db: &PluginDb, user: &User, audit: &AuditEmitter) -> CalendarRepo<ReadWrite> {
    CalendarRepo::new(db, Some(user.clone()), audit.clone())
}
