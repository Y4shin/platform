//! Process exit codes used by the CLI. Documented stable contract so shell
//! scripts and CI can distinguish "you fed me bad input" from "you called
//! something that isn't built yet".

pub const OK: i32 = 0;
/// TOML parse error or missing required file.
pub const PARSE_ERROR: i32 = 1;
/// Manifest passed parsing but failed validation rules.
pub const VALIDATION: i32 = 2;
/// Subcommand recognised but not implemented yet.
pub const NOT_IMPLEMENTED: i32 = 64;
