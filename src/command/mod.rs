mod import;
mod organize;
mod scan;

pub(crate) use self::import::ImportCommand;
pub(crate) use self::organize::OrganizeCommand;
pub(crate) use self::scan::ScanCommand;
use crate::context::AppContext;
use crate::error::Result;
use std::process::ExitCode;

pub(crate) trait Command {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode>;
}
