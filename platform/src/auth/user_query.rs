//! Loading the authenticated `User` (with memberships, permissions, and
//! user-roles) from a session id, via a small set of joins over the identity
//! tables.

use std::collections::{HashMap, HashSet};

use junius_sdk::{GroupId, Membership, Role, RoleId, User, UserId, UserRoleGrant, UserRoleId};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Resolve an unexpired session id to its `User`, or `None` if the session is
/// missing/expired.
pub async fn load_user_by_session(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Option<User>, sqlx::Error> {
    let Some(row) = sqlx::query(
        "SELECT u.id, u.email, u.display_name, u.locale \
         FROM platform.session s \
         JOIN platform.user u ON u.id = s.user_id \
         WHERE s.id = $1 AND s.expires_at > now()",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let user_id: Uuid = row.get("id");
    Ok(Some(User {
        id: UserId(user_id),
        email: row.get("email"),
        display_name: row.get("display_name"),
        locale: row.get("locale"),
        memberships: load_memberships(pool, user_id).await?,
        user_roles: load_user_roles(pool, user_id).await?,
    }))
}

async fn load_memberships(pool: &PgPool, user_id: Uuid) -> Result<Vec<Membership>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT gm.group_id, g.name AS group_name, \
                gr.id AS role_id, gr.name AS role_name, rp.permission \
         FROM platform.group_membership gm \
         JOIN platform.group g ON g.id = gm.group_id \
         JOIN platform.group_role gr ON gr.id = gm.role_id \
         LEFT JOIN platform.role_permission rp ON rp.role_id = gr.id \
         WHERE gm.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    // Group rows by group_id, preserving first-seen order, collecting permissions.
    let mut by_group: HashMap<Uuid, Membership> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();
    for row in rows {
        let group_id: Uuid = row.get("group_id");
        let membership = by_group.entry(group_id).or_insert_with(|| {
            order.push(group_id);
            Membership {
                group_id: GroupId(group_id),
                group_name: row.get("group_name"),
                role: Role {
                    id: RoleId(row.get("role_id")),
                    name: row.get("role_name"),
                },
                permissions: HashSet::new(),
            }
        });
        if let Some(permission) = row.get::<Option<String>, _>("permission") {
            membership.permissions.insert(permission);
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|g| by_group.remove(&g))
        .collect())
}

/// Load M18 user-role grants for `user_id`. The `LEFT JOIN` lets a role with
/// no permissions still appear (degenerate but observable) without breaking the
/// caller's grouping logic.
async fn load_user_roles(pool: &PgPool, user_id: Uuid) -> Result<Vec<UserRoleGrant>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT ura.role_id, ur.name AS role_name, urp.permission \
         FROM platform.user_role_assignment ura \
         JOIN platform.user_role ur ON ur.id = ura.role_id \
         LEFT JOIN platform.user_role_permission urp ON urp.role_id = ur.id \
         WHERE ura.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut by_role: HashMap<Uuid, UserRoleGrant> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();
    for row in rows {
        let role_id: Uuid = row.get("role_id");
        let grant = by_role.entry(role_id).or_insert_with(|| {
            order.push(role_id);
            UserRoleGrant {
                role_id: UserRoleId(role_id),
                role_name: row.get("role_name"),
                permissions: HashSet::new(),
            }
        });
        if let Some(permission) = row.get::<Option<String>, _>("permission") {
            grant.permissions.insert(permission);
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|r| by_role.remove(&r))
        .collect())
}
