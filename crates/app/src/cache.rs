//! Cache-database construction.

use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use rawr_cache::{Database, Repository};
use rawr_config::Config;

/// Open the cache database at `library.cache`.
///
/// Keep the [`Database`] to close it at shutdown. When `read_only` is set,
/// the repository makes no writes.
pub async fn cache(config: &Config, read_only: bool) -> Result<(Database, Repository)> {
    let database = Database::connect(config.library.cache.relative()).await.or_raise(|| ErrorKind::Cache)?;
    let repository = Repository::new(database.pool().clone(), read_only);
    Ok((database, repository))
}
