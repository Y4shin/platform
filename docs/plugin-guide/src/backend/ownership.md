# Ownership, sharing and visibility

Permissions say "may create events". This chapter is about the *instances*: who
owns a given event, who it's shared with, and who may see it. The platform keeps
a per-resource ACL in `platform.resource_principal` (ownership) and
`platform.resource_share` (shares); your plugin records into it and reads
through it.

## Record ownership atomically with creation

When you create a resource, record its owner in the **same transaction** as the
`INSERT`, via `Authz::record_owner`:

```rust
let mut tx = self.pool().begin().await?;
let row = sqlx::query!(
    "INSERT INTO events.event (...) VALUES (...) RETURNING id"
)
.fetch_one(&mut *tx)
.await?;

authz.record_owner(&mut tx, "events:event", row.id, owner).await?;
tx.commit().await?;
```

`owner` is a `Principal`:

```rust
pub enum Principal {
    User(UserId),
    Group(GroupId),
    Public,   // shares only — a resource can't be *owned* by the public
}
```

Recording in the same transaction means there's never a window where a row
exists without an owner.

## User vs. group ownership is a service decision

`events` lets the creator choose: a personal event (`Principal::User`) or a group
event (`Principal::Group`). **Who may own on a group's behalf is the *service's*
call, not the repository's.** The handler verifies the caller belongs to the
target group before passing a group principal:

```rust
"group" => {
    let gid = GroupId(parse_uuid(request.owner_id, "owner_id")?);
    if !ectx.resources.groups.is_member(gid, caller).await? {
        return Err(err::group_membership_required());
    }
    Principal::Group(gid)
}
```

### The non-obvious group-access rule

A group-owned resource is visible to a member **only if that member's role
grants the permission** (via `platform.role_permission`). Membership alone is
not enough. "Alice is in the Committee" does **not** let her see a Committee
event unless her role in that group lists `events:read`.

For per-group authorization beyond the static RPC gate, use the SDK helper
rather than walking `user.memberships` by hand:

```rust
user.has_permission_in_group(group_id, "events:write")
```

## Forget ownership on delete

Record on create, **forget on delete**. `Authz::forget_resource` clears the
`resource_principal` rows *and* any `resource_share` rows, in the same
transaction as the row delete — no orphaned ACL entries:

```rust
let mut tx = self.pool().begin().await?;
let deleted = sqlx::query!(
    "DELETE FROM events.event WHERE id = $1 RETURNING id", id
)
.fetch_optional(&mut *tx)
.await?;

authz.forget_resource(&mut tx, "events:event", id).await?;
tx.commit().await?;
```

## Sharing

Owners can grant a permission on a resource to another principal via
`Authz::share` (and revoke with `Authz::unshare`). Only the owner may share:

```rust
authz.share(
    "events:event",
    event_id,
    Principal::Group(committee_id),
    "events:read",
    None,                 // optional expiry: Option<DateTime<Utc>>
).await?;
```

A share with `Principal::Public` makes a private resource world-readable for the
granted permission — the mechanism behind a "share link". Shares flow into
`user_can_access` automatically, so a shared-in user starts seeing the resource
on their next read with no further plumbing.

## The read pattern: `visibility OR user_can_access`

Every read filters by the central access function and projects the viewer's
server-computed capabilities:

```sql
SELECT e.*,
  platform.user_can_access('events:event', e.id, $1, 'events:write') AS "viewer_can_edit!",
  platform.user_can_access('events:event', e.id, $1, 'events:share') AS "viewer_can_share!"
FROM events.event e
WHERE e.visibility = 'public'
   OR platform.user_can_access('events:event', e.id, $1, 'events:read')
```

- `$1` is the caller id (`self.user().map(|u| u.id.0)`; `NULL` for anonymous).
- **Public rows bypass the ACL**; private rows require access.
- A row the caller can't see is returned as **`NotFound`** — never
  distinguished from a missing row, so existence doesn't leak.
- `update` and `delete` gate the same way, in their `WHERE` clause:
  `WHERE id = $2 AND platform.user_can_access('events:event', id, $1, 'events:write')`.

## `viewer_can_*`: compute access once, on the server

The query returns `viewer_can_edit` / `viewer_can_share` as columns. These flow
all the way to the frontend, which uses them to decide whether to render an
"Edit" or "Share" button.

**The frontend must never recompute access.** Client-side permission gating is a
no-op stub today; the server-computed flags are the truth. This keeps a single
authority for "can this viewer do X" and avoids the classic bug where the UI
shows an action the server then refuses.

Next: surfaces that have no caller at all.
