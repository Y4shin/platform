#[cfg(feature = "source-build")]
pub mod build;
#[cfg(feature = "source-build")]
pub mod cache_cmd;
pub mod check;
pub mod check_rpc;
pub mod cross_check;
#[cfg(feature = "develop")]
pub mod dev;
pub mod i18n;
pub mod migrate;
#[cfg(feature = "develop")]
pub mod new;
pub mod plugin_cmd;
pub mod provision;
#[cfg(feature = "develop")]
pub mod rpc_scaffold;
// `sync` stays compiled even without `develop`: `build` (M11) and `plugin
// enable/disable` (M11) reuse its composition-glue generation. The `sync`
// subcommand entry point is gated by `develop`, but the module's helpers
// are needed by `build`/`enable`/`disable` so we keep them under
// `source-build` too. With both features off (the trimmed in-container CLI
// from M24), sync has no callers — gate the whole module out.
#[cfg(any(feature = "develop", feature = "source-build"))]
pub mod sync;

/// Print a "not yet implemented" message to stderr and return
/// [`exit::NOT_IMPLEMENTED`]. Only the (develop-gated) `new` subcommand still has
/// stubs that use it.
#[cfg(feature = "develop")]
pub(crate) fn not_implemented(name: &str, milestone: &str) -> i32 {
    eprintln!("junius: {name} is not yet implemented (planned for {milestone})");
    crate::exit::NOT_IMPLEMENTED
}
