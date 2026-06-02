# Testing

Junius is built **test-first**. New behaviour goes red → green → refactor: write
the test from the spec/acceptance criteria, watch it fail, write the minimum to
pass, then clean up with the suite green. A test that passes before the code
exists is wrong. This chapter shows where each kind of test lives and how to run
it.

## Pick the cheapest test that proves the behaviour

| Kind | Where | Run with | Needs |
| --- | --- | --- | --- |
| Rust unit | `#[cfg(test)]` modules | `task test:rust:unit` | nothing (DB-less) |
| Rust integration | crate `tests/` | `task test:rust` | Docker (testcontainers) |
| FE unit | `*.test.ts(x)` (vitest) | `task test:js` (also typechecks) | nothing |
| E2E | `*.spec.ts` under `e2e/` (Playwright) | `task test:e2e` | Docker (ephemeral stack) |

Prefer unit over integration over E2E. Reserve E2E for genuinely end-to-end
flows. Every acceptance criterion should map to at least one test — and that test
derives its assertions from the **spec**, never from the implementation.

## Rust unit tests

DB-less logic — encoding, validation, the iCalendar renderer — goes in
`#[cfg(test)]` modules and runs with no infrastructure:

```bash
task test:rust:unit
```

## Testing a job handler without a broker

You can unit-test an email/job handler without RabbitMQ: build a disabled `Jobs`
and a capturing `Transport`, call the handler, and assert on what it tried to
send. The worked example is
`plugins/events/tests/send_signup_confirmation.rs`:

```rust
// sketch: a Transport that records the message instead of sending it
let captured = CapturingTransport::default();
let resources = test_resources_with(/* Jobs::disabled(...), captured.clone(), … */);
send_signup_confirmation(job, resources).await.unwrap();
assert_eq!(captured.sent().len(), 1);
assert!(captured.sent()[0].subject.contains("signed up"));
```

## Rust integration tests

Tests in a crate's `tests/` directory get a real Postgres via testcontainers.
Use them for repository behaviour and ACL semantics that only a database can
prove (e.g. "a private event is `NotFound` for a non-owner"):

```bash
task test:rust        # all Rust tests (needs Docker)
```

## Frontend unit tests

`*.test.tsx` files run under vitest, which **also typechecks** — so a type error
fails the test run:

```bash
task test:js
```

Compile the catalogs first if you reference i18n (the task does this for you:
`pnpm exec lingui compile`).

## E2E tests

Real-browser, full-stack specs live under
`plugins/<name>/frontend/e2e/*.spec.ts`. Auto-discovery is glob-based — drop a
spec in and `task test:e2e` runs it; nothing to register.

```ts
// plugins/myplugin/frontend/e2e/list.spec.ts
import { expect, test } from '@junius/e2e';

test('alice sees an empty state on first visit', async ({ page, loginAs }) => {
  await loginAs('alice', { permissions: ['myplugin:read'] });
  await page.goto('/p/myplugin');
  await expect(page.getByText(/no items yet/i)).toBeVisible();
});
```

### The `@junius/e2e` harness

- **`loginAs(name, { permissions })`** seeds a session *directly into Postgres*
  (a `platform.user` upsert + a per-call group/role carrying the requested
  permissions + a `platform.session` row) and sets the `session` cookie on the
  Playwright context. No Authentik, no OIDC round-trip — a spec starts already
  logged in. Subjects use the `e2e:<name>` prefix so they can't collide with real
  Authentik subjects.
- **`db`** is a `pg.Pool` against the same ephemeral Postgres, for direct
  seed/cleanup in your spec.
- **Stack lifecycle.** `task test:e2e` spins an ephemeral Postgres + RabbitMQ +
  MinIO + mailpit via testcontainers (every container labelled
  `junius-e2e-run=<uuid>`), runs migrations, boots `juniusd` with
  `--features embed-frontend`, runs the suite, then tears it all down. Your
  `task dev` stack is never touched. Authentik is intentionally absent.

### Conventions

- Don't import another plugin's internals from a spec. Cross-plugin journeys go
  under `e2e/cross/**`.
- Retries (`retries: 2`) only apply under `CI=true`. Locally a flaky spec fails
  the first run — **fix the flake, don't paper over it**.
- `loginAs` creates a *fresh per-call* group. For two users in the *same* group
  (group-owned visibility scenarios), seed the shared group + memberships via the
  `db` fixture.
- A spec gone wrong locally can leave containers behind —
  `task test:e2e:clean` force-removes anything labelled `junius-e2e-run=*`.

## Determinism

Tests must not depend on wall-clock time, real network, or ordering beyond what
the testcontainers stack provides. A test that's flaky is a test that's wrong —
the suite is only useful if green means green.

Next: the gate that ties all of this together.
