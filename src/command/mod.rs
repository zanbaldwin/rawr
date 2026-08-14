mod duplicates;
#[cfg(feature = "_render")]
mod export;
mod import;
mod init;
mod organize;
mod scan;
mod stats;

pub(crate) use self::duplicates::DuplicatesCommand;
#[cfg(feature = "_render")]
pub(crate) use self::export::ExportCommand;
pub(crate) use self::import::ImportCommand;
pub(crate) use self::init::InitCommand;
pub(crate) use self::organize::OrganizeCommand;
pub(crate) use self::scan::ScanCommand;
pub(crate) use self::stats::StatsCommand;
use crate::context::AppContext;
use crate::error::Result;
use std::process::ExitCode;

pub(crate) trait Command {
    async fn execute(&self, ctx: &mut AppContext) -> Result<ExitCode>;
}
