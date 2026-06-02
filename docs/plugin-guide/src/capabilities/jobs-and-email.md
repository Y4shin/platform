# Background jobs and email

Some work shouldn't happen inside a request: sending an email, calling a slow
external API, processing in batches. Junius gives plugins a **background job**
system, and the canonical use is sending email asynchronously. The `events`
plugin's sign-up confirmation is the worked example.

## Declare the capabilities

Both handles are gated. Declare them in `plugin.toml`:

```toml
[requires]
capabilities = ["email.send", "job.enqueue"]
```

Without the declaration, `resources.email` / `resources.jobs` calls fail at
runtime with `CapabilityNotDeclared`. (See [The plugin manifest](../backend/manifest.md).)

## Define a job

A job is a serializable payload plus a stable name:

```rust
use serde::{Deserialize, Serialize};
use junius_sdk::Job;

#[derive(Serialize, Deserialize)]
pub struct SendSignupConfirmation {
    pub recipient_email: String,
    pub event_title: String,
    #[serde(default)]
    pub recipient_locale: Option<String>,   // captured at enqueue time — see below
}

impl Job for SendSignupConfirmation {
    const NAME: &'static str = "events.send_signup_confirmation";
}
```

## Write the handler

A handler takes the deserialized job and a **caller-less** `PluginResources`
(jobs run outside any request):

```rust
use junius_sdk::{EmailMessage, PluginError, PluginResources};

async fn send_signup_confirmation(
    job: SendSignupConfirmation,
    resources: PluginResources,
) -> Result<(), PluginError> {
    let l = resources.localizer.for_stored(job.recipient_locale.as_deref());
    resources
        .email
        .send(EmailMessage {
            to: vec![job.recipient_email],
            subject: l.t(messages::EventsSignupEmailSubject { title: &job.event_title }),
            body_text: l.t(messages::EventsSignupEmailBody { /* … */ }),
            ..Default::default()
        })
        .await
}
```

## Register the handler

Return your handlers from `Plugin::jobs()`:

```rust
fn jobs(&self) -> Vec<JobHandler> {
    vec![JobHandler::new::<SendSignupConfirmation, _, _>(send_signup_confirmation)]
}
```

The host worker dispatches each job by its `NAME` to the matching handler, with
your plugin's own resources.

## Enqueue from a handler

```rust
let _ = ectx.resources.jobs.enqueue(SendSignupConfirmation {
    recipient_email: email,
    event_title: title,
    recipient_locale: ectx.user.as_ref().and_then(|u| u.locale.clone()),
}).await;   // best-effort: don't fail the user's action if enqueue hiccups
```

`enqueue` returns a run id. For side-effects like a confirmation email, treat it
as **best-effort** — a failed enqueue shouldn't roll back the sign-up itself.

## Carry the locale through the payload

Jobs run outside the request, so there's no caller to read a locale from at
dispatch time. **Capture it at enqueue time** and carry it in the payload (the
`recipient_locale` field above). `None` falls back to the deployment default;
the worker resolves through `resources.localizer.for_stored(...)`. Same applies
to `.ics` feeds and anything else that runs detached. See
[Internationalization](./i18n.md).

## Email specifics

`resources.email.send(EmailMessage { … })` takes recipients, subject, text body,
optional HTML body, and attachments. The host:

- validates the `from` domain against the deployment's allowed sender list,
- routes through the configured transport (SMTP, Resend, mailpit in dev, or a
  log sink),
- and is gated on `email.send`.

Build subject/body through the localizer so the recipient gets their language,
not yours.

## Testing without a broker

You don't need RabbitMQ to unit-test a job handler. Construct a disabled `Jobs`
and a capturing `Transport`, call the handler directly, and assert on what it
tried to send — see `plugins/events/tests/send_signup_confirmation.rs`. The
[Testing](../quality/testing.md) chapter covers the pattern.
