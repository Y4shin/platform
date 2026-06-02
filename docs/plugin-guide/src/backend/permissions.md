# Permissions and access control

Junius enforces permissions in **three** complementary layers. Understanding all
three — and which job each does — is the key to writing a secure plugin without
fighting the type system.

| Layer | When | Mechanism | Prevents |
| --- | --- | --- | --- |
| **Runtime RPC gate** | before your handler runs | proto `(platform.v1.requires)` → host `RPC_REQUIRES` table | callers without the permission reaching the handler at all |
| **Compile-time witness** | at build time | `Has<P>` bounds on repository methods | *you* calling a mutating method from a read-only context |
| **Per-resource ACL** | inside each query | `platform.user_can_access(...)` | returning rows the caller can't see, even with the permission |

Permissions answer "may this *kind* of action happen?"; the ACL answers "for
*this specific row*?". You need both.

## Declaring the vocabulary

Permissions are declared once, in the manifest (see
[The plugin manifest](./manifest.md)):

```toml
[permissions]
"events:read"  = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."
```

`plugin_metadata!()` (invoked once in `src/lib.rs`) turns each into a zero-sized
**marker type** in a generated `permissions` module:

```text
"events:read"  →  permissions::EventsRead
"events:write" →  permissions::EventsWrite
"events:share" →  permissions::EventsShare
```

These markers implement the `Permission` trait (each carries its `NAME`), and
you compose them into **witnesses** — type-level proofs that a set of
permissions was required.

## Witnesses: `Has<P>`, `And<A, B>`, `permissions!()`

A witness is a type. The host builds one when it gates an RPC method, and hands
it to your handler as a type parameter `P`. Your repository methods constrain
`P` with `Has<X>` bounds:

```rust
// a method that only needs read access:
#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> { /* … */ }
}

// a method that needs read AND write:
#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    pub async fn create(&self, /* … */) -> Result<EventView, RepoError> { /* … */ }
}
```

Because `create` requires `Has<EventsWrite>`, a context whose witness only
proves `EventsRead` **cannot even name `create`** — the call fails to compile.
This is the compile-time gate: the wrong call is impossible to write, not merely
caught at runtime.

To build a witness by hand (rare — usually the RPC machinery does it for you)
use the `permissions!` macro:

```rust
type ReadWrite = junius_sdk::permissions!(EventsRead & EventsWrite);
// expands to And<EventsRead, And<EventsWrite, ()>>
```

> **Gotcha — don't `use junius_sdk::permissions`.** That name collides with the
> generated `permissions` *module*. Always call the macro fully qualified,
> `junius_sdk::permissions!(...)`, and import the markers from
> `crate::permissions`.

## The runtime gate comes from proto

You don't write the runtime check by hand. A proto method declares the
permission set it requires:

```proto
service EventService {
  rpc CreateEvent(CreateEventRequest) returns (CreateEventResponse) {
    option (platform.v1.requires) = "events:read,events:write";
  }
}
```

`junius sync` reads that annotation and generates the host's `RPC_REQUIRES`
table; the host checks it **before dispatch**. The same annotation drives a
per-method **type alias** (`crate::__rpc_requires::event_service::CreateEvent`)
that your handler's context parameter references — so the compile-time witness
can't drift from the runtime gate. The proto is the single source of truth; the
two layers stay in lock-step automatically. The mechanics are in
[Proto and RPC services](./rpc.md).

## The per-resource ACL: `user_can_access`

Permissions are coarse: "may read events". The ACL is fine: "may read *this*
event". Every read in your repository filters through the host's central access
function:

```sql
SELECT e.*,
  platform.user_can_access('events:event', e.id, $1, 'events:write') AS "viewer_can_edit!",
  platform.user_can_access('events:event', e.id, $1, 'events:share') AS "viewer_can_share!"
FROM events.event e
WHERE e.visibility = 'public'
   OR platform.user_can_access('events:event', e.id, $1, 'events:read')
```

`$1` is the caller id (`None`/`NULL` for anonymous). The function resolves
ownership, group-role grants and shares. Two consequences worth internalizing:

- A row the caller can't access is returned as **`NotFound`**, indistinguishable
  from a row that doesn't exist — no existence leak.
- The query also **computes the viewer's capabilities** (`viewer_can_edit`,
  `viewer_can_share`) server-side and returns them. The frontend renders affordances
  from these flags; it must **never** recompute access itself.

This pattern — `visibility OR user_can_access`, plus `viewer_can_*` projections
— recurs in every read. It's covered in depth in
[Ownership, sharing and visibility](./ownership.md).

## Group permissions are role-gated, not membership-gated

A subtle but important rule: being a *member* of a group does **not** grant you
the group's resources. The member's **role** must list the permission. "Alice is
in the Committee" does not let her see a Committee-owned event unless her role in
that group includes `events:read`.

For per-group checks beyond the static RPC gate, use the SDK helper rather than
walking memberships by hand:

```rust
if user.has_permission_in_group(group_id, "events:write") { /* … */ }
```

Same semantics as `user_can_access` step 2.

## The admin wildcard

The built-in **`admin` user-role** holds the wildcard permission `*`. A user
assigned to it passes every RPC `requires` gate and short-circuits every ACL
check — `user_can_access` returns true before evaluating ownership. **Your
plugin needs no special handling for admins**; they simply see everything.

With the model clear, the next chapter shows how repositories put the
compile-time gate and the ACL together.
