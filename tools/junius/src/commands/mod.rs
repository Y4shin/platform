pub mod build;
pub mod check;
pub mod dev;
pub mod migrate;
pub mod new;
pub mod plugin_cmd;
pub mod sync;

use crate::exit;

/// Print a "not yet implemented" message to stderr and return [`exit::NOT_IMPLEMENTED`].
pub(crate) fn not_implemented(name: &str, milestone: &str) -> i32 {
    eprintln!("junius: {name} is not yet implemented (planned for {milestone})");
    exit::NOT_IMPLEMENTED
}
