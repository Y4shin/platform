//! Files codegen'd by `junius sync` from the deployment's `platform.toml`.
//!
//! These are committed (the dev sandbox's snapshot ships in-tree) but
//! rewritten by `sync` on every run — so a deployment with a different
//! plugin set generates its own variant for that build.

pub mod domains;
