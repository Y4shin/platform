# Public and unauthenticated surfaces

Some plugin surfaces have no caller: a public event invite page anyone can open,
a sign-up form, an `.ics` calendar feed a phone polls. Junius supports these
without weakening the model — the runtime ACL (not a compile-time witness)
decides access, and the SQL `visibility = 'public' OR user_can_access(...)`
predicate does the gating.

## Why this works: pass-through sessions

The host session middleware is **pass-through**. When a session cookie is
present it attaches `Extension<User>`; otherwise it simply continues. It **never
returns 401**. So "a public surface" is just a handler that doesn't *require* a
caller — `ectx.user` is `None`, and the ACL does the rest.

There are two shapes.

## Shape 1: ungated RPC

For a public RPC method (e.g. `InviteService.GetInvite` / `Signup` / `OptOut`),
**leave the `(platform.v1.requires)` annotation off** the proto method. Its
generated alias is then `()` — the no-permission witness. The handler still
writes the alias path so `junius check` can verify the binding:

```rust
async fn get_invite(
    &self,
    ectx: EventCtx<crate::__rpc_requires::invite_service::GetInvite>,  // = ()
    request: OwnedGetInviteRequestView,
) -> ServiceResult<impl Encodable<pb::GetInviteResponse>> {
    let page = ectx.state.invites.get_page_by_slug(request.slug).await?;
    // …
}
```

Because the witness is `()`, the repository methods these handlers call must be
callable on `Repo<()>`. Put them in a **plain `impl<P>` block** — no
`#[impl_repository]`, no `Has<>` bound — so they're reachable from the
no-permission context while still using the macro's `pool()` / `user()` /
`audit()`:

```rust
impl<P> InviteRepo<P> {
    pub async fn get_page_by_slug(&self, slug: &str) -> Result<InvitePage, RepoError> {
        // SQL filters on visibility = 'public' OR user_can_access(...)
    }
}
```

See `plugins/events/src/repo/invite.rs::get_page_by_slug`.

## Shape 2: unauthenticated plain HTTP

For non-RPC endpoints — the `.ics` feeds — implement a plain Axum route in your
`Plugin::routes()` and extract `PluginResources` directly, then build a
caller-less repository:

```rust
// plugins/events/src/http.rs
async fn calendar_feed(
    resources: PluginResources,         // the host attaches per-request resources
    Path(token): Path<String>,
) -> impl IntoResponse {
    let repo = CalendarRepo::<()>::new(resources.db(), None, resources.audit.clone());
    // resolve against the *subject* (the feed token's owner), never the caller
}
```

These routes mount under `/h/<name>`. They resolve access against a **subject**
(a feed token's owner, a group) rather than a session caller.

## Frontend: the public route surface

Opt in with `public_routes = true` in `plugin.toml`, then export
`buildPublicRoutes(parent)` from your frontend's `src/index.ts`. `junius sync`
mounts it under `/i/<name>` — **outside** the authed shell — so logged-out
visitors can reach it:

```ts
export function buildPublicRoutes(parent: AnyRoute): AnyRoute[] {
  return [
    createRoute({ getParentRoute: () => parent, path: '/$slug', component: PublicInvitePage }),
  ];
}
```

On a public page, `useUser()` is non-null only when a session happens to be
present — write the page to work either way. Details in
[Routes and pages](../frontend/routes.md).

## Token-authed feeds: a bearer credential outside the session

The `.ics` subscriptions are the worked example of a **bearer credential that
isn't a session**. The design is worth copying for any "secret URL" feature:

- The feed key is **unguessable** (256-bit, base64url).
- Only its **SHA-256 hash** is stored (`calendar_token.token_hash`); the secret
  lives *only* in the URL the user holds.
- It is **scoped to one subject** and **revocable** (`revoked_at`).
- The handler hashes the incoming key, looks up the active token, and resolves
  the feed against the **subject's live entitlements** — so leaving a group or
  opting out drops events on the next poll.
- Minting a *group* feed additionally requires `events:write` within that group,
  checked from the caller's memberships at mint time.

See `src/ics.rs`, `src/repo/calendar.rs`, and `src/http.rs`.

## The throughline

In every shape above, **the SQL predicate is the access control**:
`visibility = 'public' OR user_can_access(...)`. A public invite is reachable by
anyone; a private one 404s unless a logged-in viewer has `events:read`. You never
hand-roll the "is this allowed" check in the handler — you express visibility in
the query and let the ACL decide.

That completes the backend. Next, the frontend.
