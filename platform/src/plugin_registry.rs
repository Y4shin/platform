//! Plugin registry plumbing. At M02 the registry is empty; `junius sync` from
//! M03 onward writes `platform/src/generated/plugins.rs` (a member of the
//! binary, not the library), which constructs the real `Vec<Box<dyn Plugin>>`.

use junius_sdk::Plugin;

pub type PluginVec = Vec<Box<dyn Plugin>>;

/// Empty registry. Used by integration tests that don't care about plugins.
pub fn empty() -> PluginVec {
    Vec::new()
}
