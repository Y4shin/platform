# M08 — Resource Ownership + Per-Resource Access + `viewerCanX`

> **Status:** 🚧 Planned.

## Goal

Plugins record per-instance ownership via `PluginResources.authz.record_owner(...)`. Repo list/get queries join through `platform.user_can_access(...)` so users see only resources they can access. Responses include `viewerCanEdit` / `viewerCanShare` flags computed server-side. Sharing API exists; only owners can share.

## Why now

M07 made permission *capabilities* compile-time-safe but said nothing about *which specific resources* a user can act on. The host already has the schema (laid down in M06: `resource_principal`, `resource_share`, `user_can_access(...)`). M08 wires it up at the plugin layer.

## Scope (in)

### `PluginResources.authz` API

```rust
// crates/junius-sdk/src/authz.rs

pub enum Principal {
    User(UserId),
    Group(GroupId),
    Public,
}

#[derive(Clone)]
pub struct Authz { /* opaque */ }

impl Authz {
    /// Record ownership of a freshly-created resource. Called inside the
    /// same transaction as the resource INSERT.
    pub async fn record_owner(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        resource_kind: &str,         // '<plugin>:<table>'
        resource_id: Uuid,
        owner: Principal,
    ) -> Result<(), PluginError>;

    /// Grant a permission on a resource to a principal.
    /// Only the current owner may call this; the host enforces.
    pub async fn share(
        &self,
        resource_kind: &str,
        resource_id: Uuid,
        principal: Principal,
        permission: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ShareRecord, PluginError>;

    pub async fn unshare(
        &self,
        resource_kind: &str,
        resource_id: Uuid,
        share_id: Uuid,
    ) -> Result<(), PluginError>;
}
```

Internally `Authz` holds an `Arc<PgPool>` connecting as the **host's** `platform` role (not the plugin's role), since modifying `platform.resource_principal` / `platform.resource_share` requires write access to the host schema, which plugins don't have.

The `share(...)` host-side check:

```sql
-- Allow only if the caller is the current owner.
SELECT
  CASE
    WHEN owner_user_id  = $current_user THEN true
    WHEN owner_group_id IS NOT NULL AND EXISTS (
      SELECT 1 FROM platform.group_membership
      WHERE user_id = $current_user AND group_id = owner_group_id
    ) THEN true
    ELSE false
  END AS is_owner_or_member_of_owner
FROM platform.resource_principal
WHERE resource_kind = $1 AND resource_id = $2;
```

If `is_owner_or_member_of_owner` is false → return `PermissionDenied`. (Refined "any member of the owning group can share" is the v1 behaviour; finer-grained "only the role with X capability can share" is deferred per [../design/10-infrastructure-and-data.md](../design/10-infrastructure-and-data.md) §10.7.4.)

### Hello plugin — `note` table demo

`plugins/hello/migrations/0002_note.up.sql`:

```sql
CREATE TABLE hello.note (
    id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title     TEXT NOT NULL,
    body      TEXT NOT NULL,
    created   TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Plus a permission added in `plugin.toml`:

```toml
[permissions]
"hello:read"  = "..."
"hello:write" = "..."
"hello:share" = "Share a note with another user or group."
```

`plugins/hello/src/repo/notes.rs`:

```rust
#[derive(Repository, Clone)]
pub struct NoteRepo<P = ()> {}

#[impl_repository(NoteRepo)]
impl<P: Has<HelloRead>> NoteRepo<P> {
    pub async fn list(&self) -> Result<Vec<NoteView>, RepoError> {
        sqlx::query_as!(NoteView, r#"
            SELECT
              n.*,
              platform.user_can_access('hello:note', n.id, $1, 'hello:write') AS "viewer_can_edit!",
              platform.user_can_access('hello:note', n.id, $1, 'hello:share') AS "viewer_can_share!"
            FROM hello.note n
            WHERE platform.user_can_access('hello:note', n.id, $1, 'hello:read')
        "#, self.user_id())
        .fetch_all(self.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn get(&self, id: NoteId) -> Result<Option<NoteView>, RepoError> { ... }
}

#[impl_repository(NoteRepo)]
impl<P: Has<HelloRead> + Has<HelloWrite>> NoteRepo<P> {
    pub async fn create(
        &self,
        input: NewNote,
        authz: &Authz,                        // taken as a param so the macro doesn't have to inject it
    ) -> Result<NoteView, RepoError> {
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query_as!(Note,
            "INSERT INTO hello.note (title, body) VALUES ($1, $2) RETURNING *",
            input.title, input.body
        ).fetch_one(&mut *tx).await?;

        // Plugin chooses the ownership kind. For personal notes: user-owned.
        authz.record_owner(&mut tx, "hello:note", row.id, Principal::User(self.user_id())).await?;
        tx.commit().await?;

        Ok(NoteView {
            note: row,
            viewer_can_edit: true,
            viewer_can_share: true,
        })
    }
}
```

(The `Authz` handle is passed in via the repo method rather than injected by `#[derive(Repository)]` — keeps the derive's responsibility narrow.)

### `NoteView` payload + RPC

```proto
message Note {
  string id              = 1;
  string title           = 2;
  string body            = 3;
  string created         = 4;          // RFC3339 timestamp
  bool   viewer_can_edit  = 5;
  bool   viewer_can_share = 6;
}

service NoteService {
  rpc ListNotes  (ListNotesRequest)  returns (ListNotesResponse)  { option (platform.requires) = "hello:read"; }
  rpc GetNote    (GetNoteRequest)    returns (Note)                { option (platform.requires) = "hello:read"; }
  rpc CreateNote (CreateNoteRequest) returns (Note)                { option (platform.requires) = "hello:read,hello:write"; }
  rpc ShareNote  (ShareNoteRequest)  returns (ShareNoteResponse)   { option (platform.requires) = "hello:read"; }
                                                                   // share check is at the host layer; we just need the user to "see" the note
}
```

`ShareNote` handler delegates to `ctx.resources.authz.share(...)`.

### Frontend usage

`@junius/sdk` exposes a typed helper (lives in `packages/sdk`):

```ts
// no useHasPermission for per-resource decisions; just use the flags from the server.
import { rpc } from '@junius/generated/hello';
import { useQuery, useMutation } from '@connectrpc/connect-query';

function NoteList() {
  const { data } = useQuery(rpc.NoteService.listNotes, {});
  return data?.notes.map(n => (
    <NoteCard
      note={n}
      canEdit={n.viewerCanEdit}
      canShare={n.viewerCanShare}
    />
  ));
}
```

Rule: **the FE never recomputes access locally.** If a button needs to be hidden, the server-supplied flag controls it. Cross-cuts: code review must reject FE code that computes `viewer_can_*` from the user's permission set.

### Helper macro (sketch, optional ergonomics)

A future `#[owned_by(user)]` / `#[owned_by(group_from = "field")]` attribute is sketched in this milestone's doc but **not implemented** — the explicit `authz.record_owner(...)` call is the v1 shape. The macro lands when at least three plugins request it.

## Scope (out)

- No `#[owned_by(...)]` attribute helper. Explicit form only.
- No per-resource access decisions for write methods at the SQL level — they rely on the M07 capability gate plus the `viewer_can_*` flags. A write request still goes through `platform.user_can_access(...)` via the repo's WHERE clause; mutating a row a user can't access returns a "not found" by the same join, indistinguishable from a missing row (no information leak).
- No bulk-share API. Sharing is one row at a time.
- No "any group member can share" refinement — current owner only (where owner is a user) or any current member of the owning group (where owner is a group).

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **Audit emission on share** | Yes — every `share`/`unshare` emits to `platform.audit_event` automatically inside the `Authz` host code | Compliance reading later requires it; cheap to do at the host | M13 (Speakers will emit too) |
| **`Principal` representation in `authz.share` API** | Rust enum (`User(UserId) | Group(GroupId) | Public`) | Maps cleanly to the `CHECK` constraint in `resource_share` | — |
| **Time crate** | `chrono` (already a transitive dep from sqlx/openidconnect) **OR** `time` — pick one for consistency. Default: **`chrono`** since openidconnect uses it. | Reduces duplicate time types | All later milestones |

## Open questions resolved

None new; audit logging (from M06) gets its first plugin consumer here.

## Verification

```bash
# Apply M08 migrations + sync
target/release/junius sync --config dev/platform.toml
target/release/junius migrate up --config dev/platform.toml

# Boot
target/release/junius dev --config dev/platform.toml &

# As alice — create a note
ALICE_COOKIE=$(./scripts/login.sh alice)
curl -fsS -b "$ALICE_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/CreateNote \
  -d '{"title":"private","body":"shh"}'
# → 200, returns note { ..., viewer_can_edit: true, viewer_can_share: true }
# capture the note id as $NOTE_ID

# Bob lists notes — should not see alice's
BOB_COOKIE=$(./scripts/login.sh bob)
curl -fsS -b "$BOB_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/ListNotes -d '{}'
# → {"notes":[]}

# Bob tries to fetch by ID directly — should 404 (not 403; no existence leak)
curl -i -b "$BOB_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/GetNote \
  -d "{\"id\":\"$NOTE_ID\"}"
# → 404

# Alice shares with bob (hello:read)
curl -fsS -b "$ALICE_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/ShareNote \
  -d "{\"id\":\"$NOTE_ID\",\"principal_kind\":\"user\",\"principal_id\":\"$BOB_USER_ID\",\"permission\":\"hello:read\"}"
# → 200

# Bob lists again
curl -fsS -b "$BOB_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/ListNotes -d '{}'
# → notes:[{ id: <NOTE_ID>, viewer_can_edit: false, viewer_can_share: false, ... }]

# Audit emitted
psql platform -c "SELECT event_kind, actor_user_id FROM platform.audit_event ORDER BY occurred_at DESC LIMIT 5;"
# → hello:note.share (actor=alice), hello:note.create (actor=alice)

# Bob attempts to share alice's note — denied
curl -i -b "$BOB_COOKIE" -X POST \
  http://127.0.0.1:8080/rpc/hello.v1.NoteService/ShareNote \
  -d "{\"id\":\"$NOTE_ID\",\"principal_kind\":\"public\",\"permission\":\"hello:read\"}"
# → 403

# Tests
cargo test -p hello-plugin
cargo test -p platform                  # integration: full multi-user flow
```

After M08, the platform has per-instance access control. M09 introduces a second plugin so we exercise cross-plugin composition.
