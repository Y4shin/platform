//! Library surface of the `junius` CLI. Currently exposes the migration runner
//! so it can be driven from integration tests (and, later, reused by the host).
//! The binary entry point lives in `src/main.rs`.

pub mod migrate;
