# M13 — First Real Domain Plugin: Speakers

## Goal

`plugins/speakers/` ships as the first real domain plugin: permissions `speakers:read`/`write`/`book`, tables `speaker` + `booking`, exposes `SpeakerCard` + `SpeakerPicker`, full RPC surface (`GetSpeaker`, `ListSpeakers`, `CreateSpeaker`, `UpdateSpeaker`, `BookSpeaker`, `ShareSpeaker`), resource kind `speakers:speaker` owned per-instance. A second minimal `events` plugin scaffolds enough to demonstrate that another plugin can reuse `SpeakerPicker`. This milestone also doubles as the worked example for the plugin authoring guide.

## Why now

After M00–M12, the platform is feature-complete for a single real plugin. Building Speakers is both: (a) the first thing that justifies the platform's existence, and (b) the validation that the design composes end-to-end for a non-trivial use case. The hello plugin proved the *shape*; Speakers proves the *value*.

## Scope (in)

### `plugins/speakers/`

```
plugins/speakers/
├── plugin.toml
├── Cargo.toml
├── build.rs
├── proto/
│   └── speakers/v1/
│       ├── speakers.proto
│       └── booking.proto
├── migrations/
│   ├── 0001_speaker.up.sql
│   ├── 0002_booking.up.sql
│   └── 0003_speaker_indexes.up.sql
├── src/
│   ├── lib.rs                            # plugin_metadata!(), Plugin impl, PluginCtx derive
│   ├── permissions.rs                    # (auto from macro — listed here for orientation)
│   ├── repo/
│   │   ├── mod.rs
│   │   ├── speaker.rs                    # SpeakerRepo
│   │   └── booking.rs                    # BookingRepo
│   ├── service/
│   │   ├── mod.rs
│   │   ├── speakers.rs                   # SpeakerService impl
│   │   └── booking.rs                    # BookingService impl
│   └── domain.rs                         # SpeakerId, BookingSlot, etc.
└── frontend/
    ├── package.json                      # @junius/plugin-speakers
    ├── tsconfig.json
    └── src/
        ├── index.ts                      # exports buildRoutes + components + types
        ├── routes/
        │   ├── index.ts
        │   └── pages/
        │       ├── SpeakersListPage.tsx
        │       ├── SpeakerDetailPage.tsx
        │       ├── SpeakerEditPage.tsx
        │       └── BookingsPage.tsx
        ├── lib/
        │   ├── SpeakerCard.tsx           # exposed
        │   └── SpeakerPicker.tsx         # exposed
        └── domain.ts
```

### `plugin.toml`

```toml
[plugin]
name = "speakers"
display_name = "Speakers"
description = "Manage public speakers and their booking history."
manifest_schema = 1

[exposes.components.SpeakerCard]
module      = "./frontend/src/lib/SpeakerCard"
description = "Compact speaker summary card."

[exposes.components.SpeakerPicker]
module      = "./frontend/src/lib/SpeakerPicker"
description = "Searchable speaker selector."

[exposes.tables.speaker]
schema      = "speakers"
description = "Public speaker records."

[permissions]
"speakers:read"  = "View speakers and their booking history."
"speakers:write" = "Create, edit, and delete speakers."
"speakers:book"  = "Book a speaker for an event."
"speakers:share" = "Share a speaker with another user or group."

[requires]
capabilities = ["db.read", "db.write", "audit.emit", "email.send"]
```

### Migrations

`0001_speaker.up.sql`:

```sql
CREATE TABLE speakers.speaker (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    full_name     TEXT NOT NULL,
    bio           TEXT,
    email         TEXT,
    photo_key     TEXT,                   -- references S3 attachment
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

`0002_booking.up.sql`:

```sql
-- @requires speakers:0001_speaker
CREATE TABLE speakers.booking (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    speaker_id     UUID NOT NULL REFERENCES speakers.speaker(id),
    booked_for     TIMESTAMPTZ NOT NULL,
    booked_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    booked_by      UUID NOT NULL REFERENCES platform.user(id),
    notes          TEXT,
    UNIQUE (speaker_id, booked_for)
);
```

`0003_speaker_indexes.up.sql`:

```sql
CREATE INDEX speaker_name_idx        ON speakers.speaker (full_name);
CREATE INDEX booking_speaker_idx     ON speakers.booking (speaker_id);
CREATE INDEX booking_booked_for_idx  ON speakers.booking (booked_for);
```

### Repository structure

```rust
// plugins/speakers/src/repo/speaker.rs

#[derive(Repository, Clone)]
pub struct SpeakerRepo<P = ()> {}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead>> SpeakerRepo<P> {
    pub async fn list(&self) -> Result<Vec<SpeakerView>, RepoError> {
        sqlx::query_as!(SpeakerView, r#"
            SELECT
              s.*,
              platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:write') AS "viewer_can_edit!",
              platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:share') AS "viewer_can_share!",
              platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:book')  AS "viewer_can_book!"
            FROM speakers.speaker s
            WHERE platform.user_can_access('speakers:speaker', s.id, $1, 'speakers:read')
        "#, self.user_id())
        .fetch_all(self.pool()).await.map_err(Into::into)
    }

    pub async fn get(&self, id: SpeakerId) -> Result<Option<SpeakerView>, RepoError> { ... }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersWrite>> SpeakerRepo<P> {
    pub async fn create(
        &self,
        input: NewSpeaker,
        authz: &Authz,
    ) -> Result<SpeakerView, RepoError> {
        let mut tx = self.pool().begin().await?;
        let s = sqlx::query_as!(Speaker,
            "INSERT INTO speakers.speaker (full_name, bio, email) VALUES ($1, $2, $3) RETURNING *",
            input.full_name, input.bio, input.email
        ).fetch_one(&mut *tx).await?;
        // Personal contact: user-owned.
        authz.record_owner(&mut tx, "speakers:speaker", s.id, Principal::User(self.user_id())).await?;
        tx.commit().await?;
        Ok(/* SpeakerView with viewer_can_* = true */)
    }

    pub async fn update(...) -> Result<SpeakerView, RepoError> { ... }
    pub async fn delete(...) -> Result<(), RepoError> { ... }
}

#[impl_repository(SpeakerRepo)]
impl<P: Has<SpeakersRead> + Has<SpeakersBook>> SpeakerRepo<P> {
    pub async fn book(...) -> Result<Booking, RepoError> { ... }
}
```

`BookingRepo` mirrors the same shape for bookings.

### Connect-RPC surface

```proto
// plugins/speakers/proto/speakers/v1/speakers.proto

service SpeakerService {
  rpc ListSpeakers   (ListSpeakersRequest)   returns (ListSpeakersResponse)   { option (platform.requires) = "speakers:read"; }
  rpc GetSpeaker     (GetSpeakerRequest)     returns (Speaker)                 { option (platform.requires) = "speakers:read"; }
  rpc CreateSpeaker  (CreateSpeakerRequest)  returns (Speaker)                 { option (platform.requires) = "speakers:read,speakers:write"; }
  rpc UpdateSpeaker  (UpdateSpeakerRequest)  returns (Speaker)                 { option (platform.requires) = "speakers:read,speakers:write"; }
  rpc DeleteSpeaker  (DeleteSpeakerRequest)  returns (Empty)                   { option (platform.requires) = "speakers:read,speakers:write"; }
  rpc BookSpeaker    (BookSpeakerRequest)    returns (Booking)                 { option (platform.requires) = "speakers:read,speakers:book"; }
  rpc ShareSpeaker   (ShareSpeakerRequest)   returns (ShareSpeakerResponse)    { option (platform.requires) = "speakers:read"; }
}

message Speaker {
  string id              = 1;
  string full_name       = 2;
  string bio             = 3;
  string email           = 4;
  string photo_url       = 5;             // presigned S3 URL, if photo_key set
  bool   viewer_can_edit  = 10;
  bool   viewer_can_share = 11;
  bool   viewer_can_book  = 12;
}
// ... + Booking + request/response messages
```

### Frontend pages

`SpeakersListPage`, `SpeakerDetailPage`, `SpeakerEditPage`, `BookingsPage` — straightforward TanStack Router + Connect-Query implementations using `@junius/design` components. Edit/Delete buttons gated by `viewer_can_edit`; Book button by `viewer_can_book`.

`SpeakerEditPage` route guard:

```tsx
import { requirePermissions } from '@junius/sdk';
import type { Permission } from '@junius/generated/speakers';

new Route({
  path: '/$speakerId/edit',
  beforeLoad: ({ context }) => requirePermissions<Permission>(context, ['speakers:write']),
  component: SpeakerEditPage,
});
```

### Exposed components

`SpeakerCard.tsx` — compact summary card used by Speakers' own list, and re-used by `events` plugin (see below).

`SpeakerPicker.tsx` — searchable selector. Consumers pass `{ value: SpeakerId | null, onChange: (id) => void }`. Internally calls `rpc.SpeakerService.listSpeakers` with a search query.

### `plugins/events/` — minimal cross-plugin consumer

A second plugin scaffolded just enough to verify cross-plugin reuse:

```toml
[plugin]
name = "events"
display_name = "Events"
manifest_schema = 1

[dependencies.speakers]
optional     = false
tables       = ["speaker"]
rpc_methods  = ["SpeakerService.GetSpeaker", "SpeakerService.ListSpeakers"]

[permissions]
"events:read"  = "View events."
"events:write" = "Create or edit events."
```

```sql
-- plugins/events/migrations/0001_event.up.sql
-- @requires speakers:0001_speaker
-- @requires platform:0001_users
CREATE TABLE events.event (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title          TEXT NOT NULL,
    when_at        TIMESTAMPTZ NOT NULL,
    speaker_id     UUID NULL REFERENCES speakers.speaker(id),
    organizer_id   UUID NOT NULL REFERENCES platform.user(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

```tsx
// plugins/events/frontend/src/routes/pages/EventEditPage.tsx
import { SpeakerPicker } from '@junius/plugin-speakers';

export function EventEditPage() {
  return (
    <Stack gap="md">
      <Input label="Title" />
      <Input type="datetime-local" label="When" />
      <SpeakerPicker value={speakerId} onChange={setSpeakerId} />
    </Stack>
  );
}
```

The `events` plugin's RPC service has one `CreateEvent` method; the rest is minimal.

### Worked example: plugin authoring guide

`docs/plugin-authoring-guide.md` is written for the first time, using Speakers as the worked example. Covers:
1. Scaffolding (`junius new plugin <name>`).
2. Declaring permissions in the manifest; using them in Rust + proto.
3. Writing repos with `#[derive(Repository)]` + `#[impl_repository(...)]`.
4. Recording ownership via `authz.record_owner` in repo `create`.
5. Joining through `platform.user_can_access` in list/get queries; surfacing `viewer_can_*` flags in RPC payloads.
6. Frontend routes via `buildRoutes`.
7. Exposing components in `[exposes.components]`.
8. Background jobs (booking-confirmation email).
9. Storage (speaker photo upload).
10. Dev workflow (`junius dev`, manifest changes, proto changes).
11. Common `junius check` failures and how to fix them.

This guide closes the "worked example & plugin authoring guide" open question.

### `forms` library introduction

Speakers' create/edit forms are the first non-trivial forms in the platform. M13 picks the default:

- `react-hook-form` + `zod` for schema → form binding.
- A new `@junius/design` `Form` + `FormField` wrapper combines them with the design-system inputs.

These additions land in `@junius/design`, ready for every later plugin.

## Scope (out)

- No real-world data import (CSV of existing speakers). Out of scope; a one-off script can be written when needed.
- No "speakers admin" page beyond what's in the plugin's own routes — no super-admin UI.
- No event search or filtering UI in the `events` plugin. It exists only to demonstrate cross-plugin reuse.
- No SMS/push notifications. Bookings just send an email via the M10 transport.
- No i18n. The plugin authoring guide notes the seam (`@junius/sdk`'s `t()` shim), but Speakers ships English-only.

## Library choices — confirm with user before starting

| Choice | Proposed default | Rationale | Downstream milestones to update if changed |
|---|---|---|---|
| **FE form library** | `react-hook-form` + `zod` | Pragmatic, widely-used, type-safe schema binding | Future plugins reuse this in `@junius/design`'s `Form` component |
| **Date/time inputs** | Native `<input type="datetime-local">` with a thin wrapper component | Avoids pulling in a heavy date-picker until UX demands it | — |
| **Photo upload UX** | Direct upload to `/h/speakers/photo-upload` (multipart), server stores via `Attachments`, returns key | Simple, no presigned-URL ceremony for v1 | — |
| **i18n placeholder** | `@junius/sdk` exports a no-op `t(key) => key` shim now; future i18n work plugs in the real impl | Lets plugin authors write `t('Save')` from day one without committing to an i18n library | — |

## Open questions resolved

- **Worked example & plugin authoring guide** — written, with Speakers as the worked example.
- **Internationalization** — formal punt: v1 ships English-only; the `t()` shim is the future seam.

## Verification

```bash
# Scaffold (or copy from a fixture if scaffolding doesn't cover all the structure)
target/release/junius new plugin speakers
# Manually populate the manifest, migrations, repos, services, FE pages.

target/release/junius new plugin events
# Same — minimal cross-plugin consumer.

# Sync, migrate, build
target/release/junius sync --config dev/platform.toml
target/release/junius migrate up --config dev/platform.toml
target/release/junius build

# Dev mode
target/release/junius dev --config dev/platform.toml &

# Functional verification (manual + playwright)
# 1. Log in as alice@local.
# 2. Navigate to /p/speakers → SpeakersListPage; create a speaker; appears in list.
# 3. Click into the speaker → SpeakerDetailPage shows viewer_can_edit=true; book the speaker.
# 4. Log out, log in as bob@local. /p/speakers shows nothing (no shared speakers).
# 5. As alice: share the speaker with bob (speakers:read). Bob now sees it in /p/speakers; can't edit.
# 6. Navigate to /p/events as alice; create an event; SpeakerPicker shows the speaker.
# 7. Booking-confirmation email visible in mailpit.

# Role isolation
PGUSER=role_events psql -c "SELECT id FROM speakers.speaker LIMIT 1;"     # → OK (exposed)
PGUSER=role_events psql -c "SELECT id FROM speakers.booking LIMIT 1;"     # → ERROR (not exposed)

# junius info
target/release/junius plugin info speakers
# Shows mounts, dependencies, exposed components (SpeakerCard, SpeakerPicker), exposed tables (speaker), permissions (read/write/book/share), capabilities (db.read/db.write/audit.emit/email.send).

# All checks pass
target/release/junius check

# Full test suite + E2E
cargo test --workspace
pnpm exec playwright test
```

End of M13. The platform delivers its first real domain capability. Future plugins (canvassing, events expansion, public-facing speaker directories, etc.) are now incremental work on the same chassis.
