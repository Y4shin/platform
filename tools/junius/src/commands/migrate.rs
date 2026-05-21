use super::not_implemented;
use crate::cli::MigrateCmd;

pub fn run(cmd: &MigrateCmd) -> i32 {
    let name = match cmd {
        MigrateCmd::Up => "migrate up",
        MigrateCmd::Down => "migrate down",
        MigrateCmd::Status => "migrate status",
    };
    not_implemented(name, "M06")
}
