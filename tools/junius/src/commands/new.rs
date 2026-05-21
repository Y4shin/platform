use super::not_implemented;
use crate::cli::NewCmd;

pub fn run(cmd: &NewCmd) -> i32 {
    let (name, milestone) = match cmd {
        NewCmd::Plugin { .. } => ("new plugin", "M03"),
        NewCmd::Component { .. } => ("new component", "M07"),
        NewCmd::Rpc { .. } => ("new rpc", "M05"),
        NewCmd::Migration { .. } => ("new migration", "M06"),
        NewCmd::Permission { .. } => ("new permission", "M07"),
    };
    not_implemented(name, milestone)
}
