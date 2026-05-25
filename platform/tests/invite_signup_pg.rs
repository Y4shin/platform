//! M13 Stage 7 end-to-end: invites + sign-ups over a least-privilege `role_events`
//! pool. Covers the public invite read (public event open to anonymous; private
//! event 404s without access), guest + user sign-ups, slot-limit refusal, the
//! one-per-user dedupe + opt-out/re-join, and group pre-sign-up via a member
//! snapshot. Skips cleanly without Docker.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::too_many_lines
)]

use chrono::{TimeZone, Utc};
use events_plugin::domain::{SignupStatus, Visibility};
use events_plugin::repo::{EventRepo, InviteConfig, InviteRepo, NewEvent, SignupError, SignupRepo};
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
        memberships: vec![],
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

fn invite_config(slot_limit: Option<i32>) -> InviteConfig {
    InviteConfig {
        signup_enabled: true,
        signup_open: true,
        slot_limit,
        show_title: true,
        show_datetime: true,
        show_location: true,
        show_description: true,
        show_remaining: true,
    }
}

#[tokio::test]
async fn invites_and_signups_via_role_events() {
    let node = match Postgres::default().with_tag("17-alpine").start().await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("skipping invite_signup_pg: Docker unavailable ({e})");
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
    let carol = seed_user(&admin, "carol").await;
    let committee = seed_group(&admin, "Committee", &[&alice, &carol]).await;

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
    let alice_invites: InviteRepo<ReadWrite> =
        InviteRepo::new(&db, Some(alice.clone()), audit.clone());

    // --- public event + invite (slot_limit = 2) ------------------------------
    let pub_event = alice_events
        .create(
            sample_event("Public party", Visibility::Public),
            Principal::User(alice.id),
            &authz,
        )
        .await
        .unwrap();
    let pub_invite = alice_invites
        .create(pub_event.id.0, "pub-slug", &invite_config(Some(2)))
        .await
        .unwrap();
    assert_eq!(pub_invite.slot_limit, Some(2));
    // Re-creating the invite for the same event is a unique violation.
    assert!(
        alice_invites
            .create(pub_event.id.0, "dup-slug", &invite_config(None))
            .await
            .is_err()
    );

    // Anonymous can read the public invite page.
    let anon_invites: InviteRepo<()> = InviteRepo::new(&db, None, audit.clone());
    let page = anon_invites.get_page_by_slug("pub-slug").await.unwrap();
    assert_eq!(page.title, "Public party");
    assert_eq!(page.going_count, 0);

    // --- sign-ups: guest + user, slot limit, dedupe, opt-out -----------------
    let anon_signups: SignupRepo<()> = SignupRepo::new(&db, None, audit.clone());
    let bob_signups: SignupRepo<()> = SignupRepo::new(&db, Some(bob.clone()), audit.clone());
    let carol_signups: SignupRepo<()> = SignupRepo::new(&db, Some(carol.clone()), audit.clone());

    // Guest sign-up (slot 1/2).
    anon_signups
        .signup("pub-slug", Some("Guesty"), Some("guest@local"))
        .await
        .unwrap();
    // Guest without valid details is refused.
    assert!(matches!(
        anon_signups.signup("pub-slug", Some(""), Some("x")).await,
        Err(SignupError::InvalidGuest(_))
    ));
    // Bob signs up (slot 2/2).
    let outcome = bob_signups.signup("pub-slug", None, None).await.unwrap();
    assert!(matches!(outcome.status, SignupStatus::Going));
    assert_eq!(outcome.event_title, "Public party");
    // Carol is refused — slots full.
    assert!(matches!(
        carol_signups.signup("pub-slug", None, None).await,
        Err(SignupError::Full)
    ));
    // Bob signing up again is a duplicate.
    assert!(matches!(
        bob_signups.signup("pub-slug", None, None).await,
        Err(SignupError::AlreadySignedUp)
    ));
    // Bob opts out → a slot frees up → Carol gets in.
    bob_signups.opt_out("pub-slug").await.unwrap();
    carol_signups.signup("pub-slug", None, None).await.unwrap();
    // Bob re-joining reactivates his opted-out row (still within the limit? no —
    // 2/2 now: guest + carol). Bob is refused as full.
    assert!(matches!(
        bob_signups.signup("pub-slug", None, None).await,
        Err(SignupError::Full)
    ));

    // Owner roster: guest + bob(opted_out) + carol(going).
    let roster = alice_invites.list_signups(pub_event.id.0).await.unwrap();
    assert_eq!(roster.len(), 3);
    let going = roster
        .iter()
        .filter(|s| matches!(s.status, SignupStatus::Going))
        .count();
    assert_eq!(going, 2);

    // --- private event invite: anonymous 404s, owner reads -------------------
    let priv_event = alice_events
        .create(
            sample_event("Secret meeting", Visibility::Private),
            Principal::User(alice.id),
            &authz,
        )
        .await
        .unwrap();
    alice_invites
        .create(priv_event.id.0, "priv-slug", &invite_config(None))
        .await
        .unwrap();
    assert!(matches!(
        anon_invites.get_page_by_slug("priv-slug").await,
        Err(RepoError::NotFound)
    ));
    let bob_invites: InviteRepo<()> = InviteRepo::new(&db, Some(bob.clone()), audit.clone());
    assert!(matches!(
        bob_invites.get_page_by_slug("priv-slug").await,
        Err(RepoError::NotFound)
    ));
    // Owner (Alice) can read her own private invite.
    let alice_pub_view: InviteRepo<()> = InviteRepo::new(&db, Some(alice.clone()), audit.clone());
    assert_eq!(
        alice_pub_view
            .get_page_by_slug("priv-slug")
            .await
            .unwrap()
            .title,
        "Secret meeting"
    );
    // A guest can't sign up to a private invite they can't see.
    assert!(matches!(
        anon_signups
            .signup("priv-slug", Some("X"), Some("x@local"))
            .await,
        Err(SignupError::NotFound)
    ));

    // --- group pre-sign-up: snapshot members as going, one opts out ----------
    let grp_event = alice_events
        .create(
            sample_event("Committee AGM", Visibility::Private),
            Principal::Group(committee),
            &authz,
        )
        .await
        .unwrap();
    let grp_invite = alice_invites
        .create(grp_event.id.0, "grp-slug", &invite_config(None))
        .await
        .unwrap();
    let alice_signups: SignupRepo<ReadWrite> =
        SignupRepo::new(&db, Some(alice.clone()), audit.clone());
    let inserted = alice_signups
        .presign_users(grp_invite.id, &[alice.id, carol.id])
        .await
        .unwrap();
    assert_eq!(inserted, 2);
    // Idempotent: re-running inserts nothing.
    assert_eq!(
        alice_signups
            .presign_users(grp_invite.id, &[alice.id, carol.id])
            .await
            .unwrap(),
        0
    );
    // Carol opts out of the group event.
    carol_signups.opt_out("grp-slug").await.unwrap();
    let grp_roster = alice_invites.list_signups(grp_event.id.0).await.unwrap();
    let grp_going = grp_roster
        .iter()
        .filter(|s| matches!(s.status, SignupStatus::Going))
        .count();
    assert_eq!(grp_going, 1); // only Alice remains going
}
