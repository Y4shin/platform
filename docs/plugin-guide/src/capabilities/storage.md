# Object storage

Plugins that handle files — attachments, images, exports — use the host's object
storage capability rather than touching S3/MinIO directly. The host gives you a
**logical bucket** handle with keys automatically scoped to your plugin, plus
presigned-URL support for direct browser up/downloads.

> The `events` plugin doesn't use storage, so this chapter is grounded in the
> SDK surface rather than a worked example. The `admin` and future plugins are
> the references as they adopt it.

## Declare buckets and capabilities

Two pieces in `plugin.toml`. First the capabilities you need:

```toml
[requires]
capabilities = ["storage.read", "storage.write"]
```

Then the logical buckets your plugin reads/writes:

```toml
[storage.buckets.attachments]
description = "User-uploaded attachments for records in this plugin."
```

A logical bucket is mapped to a **physical** bucket in the deployment's
`platform.toml` (`[config.storage.mapping]`). If a declared bucket isn't mapped,
`junius check`'s `STORAGE.BUCKET.UNMAPPED` rule fails the build — so a
misconfigured deployment is caught before it runs.

`plugin_metadata!()` generates a `Bucket` enum from these declarations, so you
reference buckets by a typed value rather than a string.

## Get a bucket handle

```rust
let bucket = resources.storage.bucket(Bucket::Attachments)?;
```

## Read and write

```rust
// direct put (server holds the bytes):
let object_id = bucket
    .put("invoice.pdf", bytes, "application/pdf")
    .await?;

// direct get:
let bytes = bucket.get("invoice.pdf").await?;

// delete:
bucket.delete("invoice.pdf").await?;
```

Keys are **automatically scoped** to `<plugin>/<key>`, so two plugins can both
use a key named `logo.png` without collision. `bucket.scoped_key("logo.png")`
returns the physical key if you need it (e.g. to store alongside a DB row).

## Presigned URLs for the browser

For large uploads/downloads, hand the browser a time-limited URL so bytes don't
flow through your handler:

```rust
use std::time::Duration;

// browser uploads straight to storage:
let target = bucket
    .upload_url("invoice.pdf", "application/pdf", Duration::from_secs(300))
    .await?;
// → target carries the URL + any required fields; return it to the FE

// browser downloads straight from storage:
let url = bucket
    .download_url("invoice.pdf", Duration::from_secs(300))
    .await?;
```

When the provider can't presign (or is configured not to), the host falls back
to **mediated** URLs that route through it — your code calls the same methods
either way.

## Track objects in your tables

Stored objects are recorded in `platform.object`, which your plugin tables can
reference. Store the `ObjectId` (or the scoped key) on the owning row so you can
find, serve and clean up the object alongside its record — and so a deleted
record can delete its blob.

## The capability split

| Capability | Lets you |
| --- | --- |
| `storage.read` | `get`, `download_url` |
| `storage.write` | `put`, `upload_url`, `delete` |

Declare only what the plugin actually does. A plugin that only serves previously
stored files needs `storage.read` alone.
