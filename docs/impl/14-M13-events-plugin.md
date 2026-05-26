# M13 — First Real Domain Plugin: Events

> **Status:** ✅ Implemented.
>
> *(This milestone was originally scoped as a "speakers" plugin; it was re-scoped
> to an **events** plugin — a more relatable calendar-style first real plugin that
> still exercises every platform primitive: per-instance ownership, user **and**
> group principals, public/private visibility, background jobs, email, and the
> first unauthenticated public surfaces.)*

## Goal

`plugins/events/` ships as the first real domain plugin. An **event** has a name,
a date/time (optionally a multi-day range), a place, and a description. An event
**belongs to a user or a group** and is **private or public**. An event can have
an **invite page** — a shareable page (respecting the event's visibility) that
optionally lets people **sign up**, with per-field visibility toggles, an
optional slot limit, manual open/close, and — for group events — pre-sign-up of
all group members who can then opt out. Events also **export to calendars**: a
single event as a `.ics` download, and revocable, token-authed iCalendar
**subscription feeds** (a personal feed of everything you created / joined / are
in a group for, and per-group feeds).

This is the first plugin that justifies the platform's existence, and it is the
end-to-end validation that the design composes for a non-trivial use case. The
`hello`/`greetings`/`widgets` plugins proved the *shape* and cross-plugin
*composition* (M09); `events` proves the *value*. It also doubles as the worked
example for the plugin authoring guide.

## Why now

After M00–M12 the platform is feature-complete for a real plugin: per-instance
ownership + access (M08), user/group principals (M06), Connect-RPC with permission
enforcement (M05/M07), jobs + email (M10), storage (M10), and a fully-enforced
`junius check` + CI gate (M12). `events` is the first milestone that *uses* all of
that for one coherent feature, and it surfaces one capability the platform hasn't
needed yet: a **public, unauthenticated page** (the invite page).

## Scope (in)

### `plugins/events/`

```
plugins/events/
├── plugin.toml
├── Cargo.toml
├── build.rs
├── proto/
│   └── events/v1/
│       ├── events.proto                  # EventService
│       ├── invite.proto                  # InviteService
│       └── calendar.proto                # CalendarService (feed-token management)
├── migrations/
│   ├── 0001_event.up.sql
│   ├── 0002_invite.up.sql
│   ├── 0003_signup.up.sql
│   ├── 0004_indexes.up.sql
│   └── 0005_calendar_token.up.sql
├── src/
│   ├── lib.rs                            # plugin_metadata!(), Plugin impl, PluginCtx derive
│   ├── repo/
│   │   ├── mod.rs
│   │   ├── event.rs                      # EventRepo
│   │   ├── invite.rs                     # InviteRepo
│   │   ├── signup.rs                     # SignupRepo
│   │   └── calendar.rs                   # CalendarTokenRepo + feed queries
│   ├── service/
│   │   ├── mod.rs
│   │   ├── events.rs                     # EventService impl
│   │   ├── invite.rs                     # InviteService impl
│   │   └── calendar.rs                   # CalendarService impl
│   ├── ics.rs                            # VEVENT/VCALENDAR rendering
│   ├── http.rs                           # public invite page + signup + .ics export/feeds (unauthenticated)
│   └── domain.rs                         # EventId, InviteSlug, Visibility, OwnerRef, FeedToken, …
└── frontend/
    ├── package.json                      # @junius/plugin-events
    ├── tsconfig.json
    └── src/
        ├── index.ts                      # exports buildRoutes + components + types
        ├── routes/
        │   ├── index.ts
        │   └── pages/
        │       ├── EventsListPage.tsx
        │       ├── EventDetailPage.tsx
        │       ├── EventEditPage.tsx
        │       ├── InviteManagePage.tsx  # configure invite + view sign-ups (owner)
        │       └── PublicInvitePage.tsx  # the shareable page (public for public events)
        ├── lib/
        │   ├── EventCard.tsx             # exposed
        │   └── EventPicker.tsx           # exposed
        └── domain.ts
```

### `plugin.toml`

```toml
[plugin]
name = "events"
display_name = "Events"
description = "Create events (user- or group-owned, private or public) with optional sign-up invite pages."
manifest_schema = 1

[exposes.components.EventCard]
module      = "./frontend/src/lib/EventCard"
description = "Compact event summary card."

[exposes.components.EventPicker]
module      = "./frontend/src/lib/EventPicker"
description = "Searchable event selector (for future cross-plugin reuse)."

[exposes.tables.event]
schema      = "events"
description = "Events (public ones are world-readable; private ones gated by ownership)."

[permissions]
"events:read"  = "View events you own, that belong to your groups, or that are public."
"events:write" = "Create, edit, and delete events and configure their invite pages."
"events:share" = "Share a private event with another user or group."

[requires]
capabilities = ["db.read", "db.write", "audit.emit", "email.send", "job.enqueue"]
```

> Sign-up is **not** a declared permission: on a *public* invite anyone (including
> anonymous visitors) may sign up; on a *private* invite the viewer must already
> have `events:read` access to the event. That conditional gate is handler logic,
> not a static `option (platform.requires)`. The optional event banner image
> (below) adds `storage.read`/`storage.write` if included.

### Domain model

Tables live in schema `events`; the platform's per-instance ownership
(`platform.resource_principal` via `record_owner`) is the authorization source of
truth. The schema is deliberately strict: enum-style columns are **Postgres
enums** (not free-text + `CHECK IN`), and every "polymorphic" reference is two
**real, nullable foreign keys** disambiguated by a `kind` enum + a `CHECK` (rather
than one untyped UUID). Ownership is a `User` **or** a `Group` — `owner_kind` +
`owning_user_id`/`owning_group_id`, exactly one set — and **visibility**
(`private`/`public`) is a column the read queries honor by bypassing the access
check for public rows. (Postgres enums map to Rust enums via
`#[derive(sqlx::Type)]` with `#[sqlx(type_name = "events.<enum>")]`.)

| Concept | private | public |
|---|---|---|
| **user-owned** | a draft only the owner sees — for testing before publishing | a published personal event, world-readable |
| **group-owned** | a group-internal event, visible to group members | a published group event, world-readable |

### Migrations

`0001_event.up.sql`:

```sql
-- @requires platform:0001_users
-- @requires platform:0003_groups_roles_memberships
CREATE SCHEMA IF NOT EXISTS events;

CREATE TYPE events.visibility AS ENUM ('private', 'public');
CREATE TYPE events.owner_kind AS ENUM ('user', 'group');

CREATE TABLE events.event (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title           TEXT NOT NULL CHECK (length(btrim(title)) > 0),
    description     TEXT,
    location        TEXT,
    starts_at       TIMESTAMPTZ NOT NULL,
    ends_at         TIMESTAMPTZ,                       -- NULL = single instant; set for a ranged / multi-day event
    all_day         BOOLEAN NOT NULL DEFAULT false,
    visibility      events.visibility NOT NULL DEFAULT 'private',
    owner_kind      events.owner_kind NOT NULL,
    -- Real FKs, exactly one set per owner_kind; the authoritative ACL entry is
    -- also written to platform.resource_principal via record_owner().
    owning_user_id  UUID REFERENCES platform.user(id),
    owning_group_id UUID REFERENCES platform.group(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT event_time_range CHECK (ends_at IS NULL OR ends_at >= starts_at),
    CONSTRAINT event_owner_matches_kind CHECK (
        (owner_kind = 'user'  AND owning_user_id  IS NOT NULL AND owning_group_id IS NULL)
     OR (owner_kind = 'group' AND owning_group_id IS NOT NULL AND owning_user_id  IS NULL)
    )
);
```

`0002_invite.up.sql`:

```sql
-- @requires events:0001_event
CREATE TABLE events.invite (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id        UUID NOT NULL REFERENCES events.event(id) ON DELETE CASCADE,
    slug            TEXT NOT NULL UNIQUE,            -- unguessable token; the public URL
    signup_enabled  BOOLEAN NOT NULL DEFAULT true,   -- does this invite collect sign-ups at all?
    signup_open     BOOLEAN NOT NULL DEFAULT true,   -- manual open/close (independent of slots)
    slot_limit      INTEGER,                         -- NULL = unlimited
    -- Per-field visibility toggles for the public page.
    show_title       BOOLEAN NOT NULL DEFAULT true,
    show_datetime    BOOLEAN NOT NULL DEFAULT true,
    show_location    BOOLEAN NOT NULL DEFAULT true,
    show_description  BOOLEAN NOT NULL DEFAULT true,
    show_remaining    BOOLEAN NOT NULL DEFAULT true, -- show remaining slots
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id),                               -- one invite per event (v1)
    CONSTRAINT invite_slug_nonempty   CHECK (length(slug) > 0),
    CONSTRAINT invite_slot_limit_pos  CHECK (slot_limit IS NULL OR slot_limit > 0)
);
```

`0003_signup.up.sql`:

```sql
-- @requires events:0002_invite
-- @requires platform:0001_users
CREATE TYPE events.signup_kind   AS ENUM ('user', 'guest');
CREATE TYPE events.signup_status AS ENUM ('going', 'opted_out');

CREATE TABLE events.signup (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invite_id    UUID NOT NULL REFERENCES events.invite(id) ON DELETE CASCADE,
    kind         events.signup_kind NOT NULL,
    user_id      UUID REFERENCES platform.user(id),
    guest_name   TEXT,
    guest_email  TEXT,
    status       events.signup_status NOT NULL DEFAULT 'going',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 'user' sign-ups carry a user_id and no guest fields; 'guest' sign-ups carry
    -- both guest fields and no user_id. Mutually exclusive; no half-filled guests.
    CONSTRAINT signup_identity CHECK (
        (kind = 'user'  AND user_id IS NOT NULL
                        AND guest_name IS NULL AND guest_email IS NULL)
     OR (kind = 'guest' AND user_id IS NULL
                        AND guest_name  IS NOT NULL AND length(btrim(guest_name)) > 0
                        AND guest_email IS NOT NULL AND guest_email LIKE '%_@_%')
    )
);

-- One sign-up per logged-in user per invite; guests are not deduped.
CREATE UNIQUE INDEX signup_one_per_user
    ON events.signup (invite_id, user_id)
    WHERE kind = 'user';
```

`0004_indexes.up.sql`:

```sql
CREATE INDEX event_owner_user_idx  ON events.event (owning_user_id)  WHERE owning_user_id  IS NOT NULL;
CREATE INDEX event_owner_group_idx ON events.event (owning_group_id) WHERE owning_group_id IS NOT NULL;
CREATE INDEX event_visibility_idx  ON events.event (visibility);
CREATE INDEX event_starts_at_idx   ON events.event (starts_at);
CREATE INDEX signup_invite_idx     ON events.signup (invite_id);
```

`0005_calendar_token.up.sql` (backs the keyed private feeds + the group public toggle):

```sql
-- @requires platform:0001_users
-- @requires platform:0003_groups_roles_memberships
CREATE TYPE events.feed_kind AS ENUM ('personal', 'group');

-- Secret keys for the private feeds (/ics/u/<key> and private /ics/g/<name>/<key>).
CREATE TABLE events.calendar_token (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash        TEXT NOT NULL UNIQUE,        -- SHA-256 of <key>; the secret lives only in the URL
    kind              events.feed_kind NOT NULL,
    subject_user_id   UUID REFERENCES platform.user(id),
    subject_group_id  UUID REFERENCES platform.group(id),
    created_by        UUID NOT NULL REFERENCES platform.user(id),
    label             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at        TIMESTAMPTZ,                 -- soft-revoke; a revoked key stops serving
    CONSTRAINT token_subject_matches_kind CHECK (
        (kind = 'personal' AND subject_user_id  IS NOT NULL AND subject_group_id IS NULL)
     OR (kind = 'group'    AND subject_group_id IS NOT NULL AND subject_user_id  IS NULL)
    )
);

-- Per-group opt-in: when present + public, /ics/g/<name> serves with no key.
CREATE TABLE events.group_calendar (
    group_id    UUID PRIMARY KEY REFERENCES platform.group(id),
    public      BOOLEAN NOT NULL DEFAULT false,
    updated_by  UUID NOT NULL REFERENCES platform.user(id),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

> The intra-plugin `ON DELETE CASCADE`s stay inside `events`, so `junius check`'s
> `FK.CROSS.CASCADE` (which only forbids cascades *across* plugin boundaries) is
> satisfied. The cross-schema FKs into `platform.user`/`platform.group` are
> nullable, declare `-- @requires`, and use `RESTRICT` (no cross-schema cascade).

### Repository structure

```rust
// plugins/events/src/repo/event.rs

#[derive(Repository, Clone)]
pub struct EventRepo<P = ()> {}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead>> EventRepo<P> {
    /// Public events (world-readable) ∪ private events the caller can access.
    pub async fn list(&self) -> Result<Vec<EventView>, RepoError> {
        sqlx::query_as!(EventView, r#"
            SELECT
              e.*,
              platform.user_can_access('events:event', e.id, $1, 'events:write') AS "viewer_can_edit!",
              platform.user_can_access('events:event', e.id, $1, 'events:share') AS "viewer_can_share!"
            FROM events.event e
            WHERE e.visibility = 'public'
               OR platform.user_can_access('events:event', e.id, $1, 'events:read')
            ORDER BY e.starts_at
        "#, self.user_id())
        .fetch_all(self.pool()).await.map_err(Into::into)
    }

    pub async fn get(&self, id: EventId) -> Result<Option<EventView>, RepoError> { /* same predicate */ }
}

#[impl_repository(EventRepo)]
impl<P: Has<EventsRead> + Has<EventsWrite>> EventRepo<P> {
    pub async fn create(&self, input: NewEvent, authz: &Authz) -> Result<EventView, RepoError> {
        // owner is the caller (User) or a group they belong to (Group) — verified here.
        let mut tx = self.pool().begin().await?;
        let e = sqlx::query_as!(Event, r#"
            INSERT INTO events.event
              (title, description, location, starts_at, ends_at, all_day, visibility,
               owner_kind, owning_user_id, owning_group_id)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) RETURNING *
        "#, /* … */ ).fetch_one(&mut *tx).await?;
        authz.record_owner(&mut tx, "events:event", e.id, input.owner_principal).await?;
        tx.commit().await?;
        Ok(/* EventView */)
    }

    pub async fn update(...) -> Result<EventView, RepoError> { ... }   // incl. publish (private→public)
    pub async fn delete(...) -> Result<(), RepoError> { ... }
}
```

`InviteRepo` configures the invite (create/update toggles, slot limit, open/close,
`get_by_slug`, `list_signups`). `SignupRepo` handles sign-up / opt-out with
**slot enforcement in a transaction** (count `status = 'going'` against
`slot_limit` under a row lock on the invite) and **group pre-sign-up** (on invite
creation for a group event, insert a `going` row per current group member; members
opt out by flipping their row to `opted_out`).

### Connect-RPC surface

```proto
// plugins/events/proto/events/v1/events.proto
service EventService {
  rpc ListEvents  (ListEventsRequest)  returns (ListEventsResponse) { option (platform.requires) = "events:read"; }
  rpc GetEvent    (GetEventRequest)    returns (Event)              { option (platform.requires) = "events:read"; }
  rpc CreateEvent (CreateEventRequest) returns (Event)              { option (platform.requires) = "events:read,events:write"; }
  rpc UpdateEvent (UpdateEventRequest) returns (Event)              { option (platform.requires) = "events:read,events:write"; }
  rpc DeleteEvent (DeleteEventRequest) returns (Empty)             { option (platform.requires) = "events:read,events:write"; }
  rpc ShareEvent  (ShareEventRequest)  returns (ShareEventResponse) { option (platform.requires) = "events:read,events:share"; }
}

// plugins/events/proto/events/v1/invite.proto
service InviteService {
  // Owner-side configuration + sign-up management (permission-gated).
  rpc CreateInvite  (CreateInviteRequest)  returns (Invite)              { option (platform.requires) = "events:read,events:write"; }
  rpc UpdateInvite  (UpdateInviteRequest)  returns (Invite)              { option (platform.requires) = "events:read,events:write"; }
  rpc ListSignups   (ListSignupsRequest)   returns (ListSignupsResponse) { option (platform.requires) = "events:read"; }

  // Viewer/attendee side. No static `requires`: public invites are reachable by
  // anyone; for private invites the handler requires events:read access.
  rpc GetInvite     (GetInviteRequest)     returns (InvitePage);   // by slug; returns only toggled-visible fields
  rpc Signup        (SignupRequest)        returns (Signup);       // honors signup_enabled/open + slot_limit + visibility
  rpc OptOut        (OptOutRequest)        returns (Empty);        // cancel / opt out of a sign-up
}

// plugins/events/proto/events/v1/calendar.proto
// Manages subscription feed tokens (the .ics endpoints themselves are
// unauthenticated HTTP — see "Calendar export" below).
service CalendarService {
  rpc GetPersonalFeed   (Empty)                    returns (FeedUrl)            { option (platform.requires) = "events:read"; }              // mint-or-return the caller's /ics/u/<key>
  rpc CreateGroupKey    (CreateGroupKeyRequest)    returns (FeedUrl)            { option (platform.requires) = "events:read,events:write"; } // /ics/g/<name>/<key>; caller needs events:write in the group
  rpc SetGroupPublic    (SetGroupPublicRequest)    returns (Empty)             { option (platform.requires) = "events:read,events:write"; } // toggle group_calendar.public (keyless /ics/g/<name>)
  rpc ListFeeds         (Empty)                    returns (ListFeedsResponse)  { option (platform.requires) = "events:read"; }
  rpc RevokeFeed        (RevokeFeedRequest)        returns (Empty)             { option (platform.requires) = "events:read"; }              // group keys additionally require events:write in the group
}

message Event {
  string id          = 1;
  string title       = 2;
  string description = 3;
  string location    = 4;
  string starts_at   = 5;   // RFC3339
  string ends_at     = 6;   // RFC3339, empty if unset
  bool   all_day     = 7;
  string visibility  = 8;   // "private" | "public"
  string owner_kind  = 9;   // "user" | "group"
  string owner_id    = 10;
  bool   viewer_can_edit  = 20;
  bool   viewer_can_share = 21;
}
// InvitePage exposes only the fields whose show_* toggle is on, plus
// signup_open + remaining-slot count when show_remaining is set.
```

### Public invite pages (the new seam: unauthenticated access)

The invite page is the one place the platform serves content to **logged-out**
visitors. Each invite has an unguessable `slug`; the shareable URL resolves to a
**public route** that renders `PublicInvitePage`, backed by `GetInvite`/`Signup`:

- **Public event** → page + sign-up reachable by anyone. An anonymous sign-up
  captures `guest_name` + `guest_email`; a logged-in visitor's sign-up links their
  `user_id`.
- **Private event** → the page requires an authenticated viewer with `events:read`
  access to the event; otherwise it 404s (no existence leak, mirroring M08).
- Only the fields whose `show_*` toggle is on are returned/rendered; `signup`
  buttons appear only when `signup_enabled && signup_open` and (if `slot_limit` is
  set) slots remain.

This unauthenticated path is served through the plugin's HTTP namespace
(`src/http.rs`, mounted under the plugin's `http_prefix` = `/h/events`).
**Resolved (no host change needed):** the host's session middleware is
*pass-through* — it attaches `Extension<User>` when a valid session cookie is
present and otherwise just continues (it never returns 401, which is why
`/h/hello/ping` answers `200` with no login). So a public handler simply *doesn't*
extract the user / `PluginCtx`, while gated handlers do. Public handlers operate
caller-lessly on the plugin's pool (the M10 `system_context` pattern).

### Calendar export (iCalendar)

Read-only iCalendar (`.ics`) export, served from the plugin's HTTP namespace under
`/h/events/ics/…`. Rendering is shared (`src/ics.rs` → `VCALENDAR`/`VEVENT`); the
access decision differs per endpoint. The feed URLs (`/u/<key>`, group feeds) are
designed for **external calendar apps** (Google/Apple/Outlook), which poll without
a session — so a private feed carries an unguessable **key in the URL** as its
bearer credential. Keys are minted through `CalendarService` (authenticated),
stored **hashed** (`calendar_token.token_hash`; the secret only ever lives in the
URL), **scoped** to one subject, and **revocable** (`revoked_at`). All feed
queries are resolved against the **subject's live entitlements**, never the
anonymous caller's.

| Endpoint | Scope | Auth |
|---|---|---|
| `GET /h/events/ics/e/<id>` | one event | the **event ACL** — public events are open; private events need a session with `events:read` access, else `404` |
| `GET /h/events/ics/u/<key>` | the key owner's personal feed | the **key** (per-user, hashed, revocable) |
| `GET /h/events/ics/g/<name>` | a group's events | open **only if** the group opted its feed public (`group_calendar.public`); otherwise `404` |
| `GET /h/events/ics/g/<name>/<key>` | a group's events | the **key** (per-group, hashed, revocable) |

**Single event** (`/ics/e/<id>`) — a one-shot "Add to calendar" download (the
button also appears on the public invite page); reuses the event ACL verbatim, no
new surface.

**Personal feed** (`/ics/u/<key>`) — every event the key's owner *created*, is
*part of by group membership*, or has *signed up for*, computed live:

```sql
SELECT e.* FROM events.event e
WHERE platform.user_can_access('events:event', e.id, $subject, 'events:read')   -- owned, or group-member
   OR EXISTS (                                                                   -- signed up (still going)
     SELECT 1 FROM events.invite i JOIN events.signup s ON s.invite_id = i.id
     WHERE i.event_id = e.id AND s.user_id = $subject AND s.status = 'going')
```

Because membership / sign-up are evaluated **at poll time**, leaving a group or
opting out drops those events on the next refresh. A user mints/revokes only their
own personal key (`events:read`).

**Group feed** (`/ics/g/<name>` or `/ics/g/<name>/<key>`) — all events owned by
that group (`WHERE owner_kind = 'group' AND owning_group_id = $group`), including private
group events. `<name>` is the group's platform name, resolved to its id via a
host group-directory lookup (the SDK `Users`/groups accessor — **confirm a
by-name lookup exists**; otherwise the URL falls back to the group id). Two ways
in: a **public** feed (`group_calendar.public = true`, no key — externally
subscribable by anyone) or a **keyed** feed (works even while private). Both
publishing the group's calendar publicly and minting a group key require
`events:write` **within that group**.

**Security properties.** Single-event export = the event ACL. Feeds: unguessable
keys, hashed at rest, scoped to one subject, independently revocable, resolved
against the subject's live entitlements — a feed never leaks more than its subject
can currently see. The inherent trade-off, surfaced in the UI: **anyone holding a
feed URL (key, or a public group name) sees that feed**, exactly like a Google
Calendar "secret address" — hence unguessable + revocable keys, and the
`events:write` gate on publishing/keying a *group* calendar.

### Frontend pages

`EventsListPage`, `EventDetailPage`, `EventEditPage`, `InviteManagePage` — TanStack
Router + Connect-Query using `@junius/design`. Create/Edit/Delete and invite
configuration gated by `viewer_can_edit`; owner selection (me vs a group I'm in)
on the edit page; a publish toggle for private→public. `PublicInvitePage` is a
standalone, login-optional page.

`EventEditPage` route guard:

```tsx
import { requirePermissions } from '@junius/sdk';
import type { Permission } from '@junius/generated/events';

new Route({
  path: '/$eventId/edit',
  beforeLoad: ({ context }) => requirePermissions<Permission>(context, ['events:write']),
  component: EventEditPage,
});
```

### Exposed components

- `EventCard.tsx` — compact event summary (title, when, place), used by the list
  and reuse-ready for a future consumer (e.g. a public directory or a dashboard).
- `EventPicker.tsx` — searchable selector (`{ value: EventId | null, onChange }`),
  calling `rpc.EventService.listEvents`. Exposed for cross-plugin reuse, the same
  pattern M09 already validated with `VenuePicker`/`SpeakerPicker`.

No second consumer plugin is scaffolded — cross-plugin composition is already
proven by M09 (`greetings` → `hello`/`widgets`). M13 spends its complexity budget
on domain depth (visibility, invites, sign-ups) instead.

### Background job + email

Sign-ups send a confirmation email via an **M10 background job**
(`job.enqueue` → worker → `email.send`): a sign-up enqueues a
`SendSignupConfirmation` job that emails the attendee (and optionally notifies the
event owner). This is the jobs + email worked example for the authoring guide.

### Optional: event banner image (storage)

An optional `image_key TEXT` on `events.event` + a direct upload handler exercises
M10 storage (and adds `storage.read`/`storage.write` to the manifest). Included as
the authoring guide's storage example; may be deferred if the milestone is running
long (the core feature doesn't depend on it).

### Worked example: plugin authoring guide

`docs/plugin-authoring-guide.md` is written for the first time, using `events` as
the worked example. Covers:
1. Scaffolding (`junius new plugin <name>`).
2. Declaring permissions in the manifest; using them in Rust + proto.
3. Repos with `#[derive(Repository)]` + `#[impl_repository(...)]`.
4. Recording ownership via `authz.record_owner` for **user *and* group** principals.
5. The `visibility OR user_can_access` read pattern; surfacing `viewer_can_*` flags.
6. A public, unauthenticated handler (the invite page) and its access rules.
7. Frontend routes via `buildRoutes`; a login-optional public page.
8. Exposing components in `[exposes.components]`.
9. Background jobs + email (sign-up confirmation).
10. Storage (optional banner image).
11. Token-authed public endpoints (the calendar subscription feeds): minting,
    hashing, scoping, and revoking a bearer credential outside the session.
12. Common `junius check` failures and how to fix them.

This closes the "worked example & plugin authoring guide" open question.

### `forms` library introduction

The event create/edit and invite-configuration forms are the first non-trivial
forms in the platform. M13 picks the default:

- `react-hook-form` + `zod` for schema → form binding.
- A new `@junius/design` `Form` + `FormField` wrapper combining them with the
  design-system inputs (including the date/time range control).

These land in `@junius/design`, ready for every later plugin.

## Scope (out)

- **No recurring events / RRULE.** Single events with an optional multi-day range
  only. Recurrence is a substantial feature for a later milestone.
- **No calendar/timeline UI.** A simple sorted list + detail pages; no month/week
  grid view.
- **No invitations by email / RSVP tokens per person.** Sign-up is via the shared
  invite link, not personalized email invitations.
- **No waitlist** when slots are full — sign-up simply closes.
- **No two-way calendar sync / CalDAV / writable calendars.** Export is read-only
  iCalendar — single-event `.ics` + subscription feeds (see Calendar export); the
  platform never ingests or writes back external calendars.
- **No SMS/push.** Confirmations are email via the M10 transport.
- **No i18n.** `events` ships plain **English-only** strings (frontend and backend
  — UI, emails, iCalendar fields, errors). Internationalization is the prioritized
  next milestone, [M14](16-M14-internationalization.md), which introduces the seam
  and retrofits this plugin's strings; M13 does **not** pre-add a `t()` shim.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Notes |
|---|---|---|---|
| **Unauthenticated invite access** | Serve the public invite page + sign-up through the plugin's HTTP namespace (`/h/events/i/<slug>`); private events fall back to the session-gated access check | Keeps the rest of the surface session-gated; reuses the HTTP handler path | **Resolved:** the host session middleware is pass-through (attaches `Extension<User>` if present, never 401s), so public `/h/` handlers just don't extract a user — no host change needed |
| **Public visibility model** | A `visibility` column the read queries bypass the ACL for, rather than modeling "public" as a share-to-everyone in `resource_share` | Simpler + cheaper than a synthetic "everyone" principal; keeps the ACL for genuine private sharing | — |
| **Group pre-sign-up** | Snapshot the owner group's *current* members at invite creation as `going`; they opt out individually. Members added later are **not** auto-added (v1) | Predictable; avoids a live membership join on every page load | — |
| **Slug generation** | Random URL-safe token (≥128 bits) | Unguessable shareable link without leaking IDs | — |
| **FE form library** | `react-hook-form` + `zod` | Pragmatic, type-safe schema binding; reused via `@junius/design`'s `Form` | — |
| **Date/time inputs** | Native `<input type="datetime-local">` + a thin start/end "range" wrapper (+ an all-day toggle) | Avoids a heavy date-picker until UX demands it | — |
| **iCalendar generation** | The `icalendar` Rust crate (or a small hand-rolled `VCALENDAR`/`VEVENT` writer — the format is simple) | Read-only export; no need for a parser | — |
| **Calendar feed auth** | URL scheme `/ics/e/<id>` (event ACL), `/ics/u/<key>` (personal), `/ics/g/<name>` (public) / `/ics/g/<name>/<key>` (private). Keys: unguessable, hashed, scoped, revocable; resolved against the subject's live entitlements | Calendar apps poll without a session, so private feeds need a key in the URL (the "secret address" pattern); public group feeds + own/public single events need none | Publishing a group feed public + minting a group key both require `events:write` in the group |
| **Group name in feed URLs** | `/ics/g/<name>` uses the group's platform name, resolved to its id via a host group-directory lookup | Human-readable, shareable | **Confirm** the SDK exposes a group-by-name lookup; else use the group id in the URL |
| **Event banner image** | Optional; direct multipart upload → `Attachments` → `image_key` | Exercises storage + the guide's storage step | May be deferred |
| **i18n** | None in M13 — plain English strings (frontend + backend) | i18n is its own prioritized next milestone ([M14](16-M14-internationalization.md)), which owns the seam + retrofits `events` | No `t()` shim added here |

## Open questions resolved

- **Worked example & plugin authoring guide** — written, with `events` as the
  worked example (the first user-*and*-group-owned, public-*and*-private domain).
- **Unauthenticated public surfaces** — first introduced here: the invite page
  (visibility-gated) and the calendar feeds (`/ics/e/<id>` by ACL, `/ics/u/<key>`
  and private `/ics/g/<name>/<key>` by key, public `/ics/g/<name>` by group opt-in).
  The `/h/` session middleware is pass-through, so no host change is needed.
- **Internationalization** — M13 is English-only (frontend + backend); i18n is the
  prioritized next milestone, [M14](16-M14-internationalization.md), which owns the
  whole seam and migrates `events`' strings.

## Verification

```bash
# Scaffold (or copy from a fixture if scaffolding doesn't cover the full structure)
target/release/junius new plugin events
# Populate the manifest, migrations, repos, services, http handlers, FE pages.

# Sync, migrate, build
target/release/junius sync --config dev/platform.toml
target/release/junius migrate up --config dev/platform.toml
target/release/junius build

# Dev mode
target/release/junius dev --config dev/platform.toml &

# Functional verification (manual + playwright)
# 1. Log in as alice@local; /p/events → create a PRIVATE user event (a draft).
# 2. bob@local sees nothing (private, not shared, not public).
# 3. alice publishes it (private→public); bob now sees it; cannot edit.
# 4. alice creates a GROUP event for a group she's in; other group members see it; non-members don't.
# 5. alice adds an invite: enable sign-up, slot_limit=2, hide the description, leave the rest visible.
# 6. Open the invite link in a logged-out browser → public event shows the toggled fields; description hidden.
# 7. Sign up anonymously (name+email); sign up as bob; the 3rd sign-up is refused (slots full).
# 8. alice closes sign-up manually; the button disappears.
# 9. Group event invite with "pre-sign-up members": all members appear as going; one opts out.
# 10. Sign-up confirmation email visible in mailpit (sent via the background job).
# 11. Private event invite link in a logged-out browser → 404 (no existence leak); visible once logged in with access.

# Calendar export
# 12. "Add to calendar" on a PUBLIC event → /h/events/ics/e/<id> downloads a valid VEVENT (works logged out).
# 13. Same for a PRIVATE event → 404 when logged out / without access; OK for the owner.
# 14. As alice, GetPersonalFeed → subscribe a calendar app to /h/events/ics/u/<key>: shows alice's own + group + signed-up events.
#     bob opts out / alice removes bob from the group → those events drop from bob's feed on the next poll.
# 15. RevokeFeed → /h/events/ics/u/<key> now returns 404/410.
# 16. A group member with events:write runs SetGroupPublic(true) → /h/events/ics/g/<name> serves with no key (works logged out).
#     A read-only member is refused SetGroupPublic / CreateGroupKey; private /ics/g/<name> (no key) → 404.

# Role isolation (events.event is exposed; the rest of the schema is not)
PGUSER=role_events  psql -c "SELECT id FROM events.event LIMIT 1;"          # → OK (own schema)
PGUSER=role_hello   psql -c "SELECT id FROM events.event LIMIT 1;"          # → OK (exposed)
PGUSER=role_hello   psql -c "SELECT id FROM events.signup LIMIT 1;"         # → ERROR (not exposed)
PGUSER=role_hello   psql -c "SELECT id FROM events.calendar_token LIMIT 1;" # → ERROR (not exposed)
PGUSER=role_hello   psql -c "SELECT 1 FROM events.group_calendar LIMIT 1;"  # → ERROR (not exposed)

# junius info + full gate
target/release/junius plugin info events
target/release/junius check
cargo test --workspace
pnpm exec playwright test
```

End of M13. The platform delivers its first real domain capability — events with
user/group ownership, public/private visibility, public sign-up invite pages, and
revocable iCalendar export/subscription feeds — and the plugin authoring guide is
written against it. Future plugins build on the same chassis.
