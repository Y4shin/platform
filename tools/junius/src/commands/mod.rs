pub mod build;
pub mod check;
pub mod cross_check;
#[cfg(feature = "develop")]
pub mod dev;
pub mod migrate;
#[cfg(feature = "develop")]
pub mod new;
pub mod plugin_cmd;
// `sync` stays compiled even without `develop`: `build` (and, in M11, `plugin
// enable/disable`) reuse its composition-glue generation. Only the `sync`
// subcommand entry point is gated.
pub mod sync;

use crate::exit;

/// Print a "not yet implemented" message to stderr and return [`exit::NOT_IMPLEMENTED`].
pub(crate) fn not_implemented(name: &str, milestone: &str) -> i32 {
    eprintln!("junius: {name} is not yet implemented (planned for {milestone})");
    exit::NOT_IMPLEMENTED
}
