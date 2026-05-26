pub mod build;
pub mod cache_cmd;
pub mod check;
pub mod cross_check;
#[cfg(feature = "develop")]
pub mod dev;
pub mod i18n;
pub mod migrate;
#[cfg(feature = "develop")]
pub mod new;
pub mod plugin_cmd;
// `sync` stays compiled even without `develop`: `build` (and, in M11, `plugin
// enable/disable`) reuse its composition-glue generation. Only the `sync`
// subcommand entry point is gated.
pub mod sync;

/// Print a "not yet implemented" message to stderr and return
/// [`exit::NOT_IMPLEMENTED`]. Only the (develop-gated) `new` subcommand still has
/// stubs that use it.
#[cfg(feature = "develop")]
pub(crate) fn not_implemented(name: &str, milestone: &str) -> i32 {
    eprintln!("junius: {name} is not yet implemented (planned for {milestone})");
    crate::exit::NOT_IMPLEMENTED
}
