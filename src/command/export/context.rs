use crate::context::{AppContext, BackendPurpose};
use crate::error::Result;
use rawr_cache::Repository;
use rawr_config::models::FandomConfig;
use rawr_library::PathGenerator;
use rawr_output::Output;
use rawr_render::StyleConfig;
use rawr_storage::BackendHandle;
use std::{str::FromStr, sync::Arc};

/// A user-supplied reference to a work for export.
#[derive(Clone, Debug)]
pub(crate) enum WorkRef {
    /// Just a work ID — export the best version.
    BestWork(u64),
    /// Work ID with CRC32 hash — export a specific version.
    /// The u32 is parsed from 8 hex digits (e.g., `12345@37bc3355`).
    /// Matched against `Version.crc32` during resolution.
    WorkVersion(u64, u32),
    /// File path — export the file at this path.
    FilePath(String),
}
impl FromStr for WorkRef {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Ok(id) = s.parse::<u64>() {
            return Ok(WorkRef::BestWork(id));
        }
        if let Some((id_str, hex)) = s.split_once('@')
            && let Ok(id) = id_str.parse::<u64>()
            && let Ok(crc) = u32::from_str_radix(hex, 16)
        {
            return Ok(WorkRef::WorkVersion(id, crc));
        }
        Ok(WorkRef::FilePath(s.to_string()))
    }
}

pub(crate) struct ExportContext<'a> {
    pub(crate) load: BackendHandle,
    pub(crate) save: BackendHandle,
    pub(crate) cache: Arc<Repository>,
    pub(crate) styles: StyleConfig,
    pub(crate) path_generator: PathGenerator,
    pub(crate) output: Arc<dyn Output>,
    pub(crate) fandoms: &'a FandomConfig,
}
impl<'a> ExportContext<'a> {
    pub(crate) async fn try_from_app(ctx: &'a AppContext) -> Result<Self> {
        let fandoms = ctx.config.fandoms.clone();
        let path_generator = ctx.config.library.path_templates.export.parse::<PathGenerator>()?;
        let path_generator = path_generator.with_fandom_selector(move |fandom_list| {
            let names: Vec<&str> = fandom_list.iter().map(|f| f.name.as_str()).collect();
            fandoms.preferred_fandom(&names).map(String::from)
        });

        Ok(Self {
            load: ctx.get_backend_by_purpose(BackendPurpose::Import).await?.ok_or_else(|| {
                miette::miette!(
                    "No import target configured. Define one in your config file under `library.targets.import`."
                )
            })?,
            save: ctx.get_backend_by_purpose(BackendPurpose::Export).await?.ok_or_else(|| {
                miette::miette!(
                    "No export target configured. Define one in your config file under `library.targets.export`."
                )
            })?,
            cache: Arc::new(ctx.cache.clone()),
            styles: ctx.config.library.styles.iter().try_fold(StyleConfig::new(), |c, i| {
                if let Some(n) = i.strip_prefix("builtin:") { c.with_builtin(n) } else { c.with_file(i) }
            })?,
            path_generator,
            output: Arc::clone(&ctx.output),
            fandoms: &ctx.config.fandoms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_work_id() {
        let r: WorkRef = "12345".parse().unwrap();
        assert!(matches!(r, WorkRef::BestWork(12345)));
    }

    #[test]
    fn parse_work_id_version() {
        let r: WorkRef = "12345@37bc3355".parse().unwrap();
        assert!(matches!(r, WorkRef::WorkVersion(12345, 0x37bc3355)));
    }

    #[test]
    fn parse_file_path() {
        let r: WorkRef = "fandom/work.html.bz2".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(p) if p == "fandom/work.html.bz2"));
    }

    #[test]
    fn parse_bare_at() {
        let r: WorkRef = "12345@".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_non_hex_hash() {
        let r: WorkRef = "12345@zzzz".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_non_numeric_id() {
        let r: WorkRef = "abc@1234".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }

    #[test]
    fn parse_overflow_hash() {
        let r: WorkRef = "12345@fffffffff".parse().unwrap();
        assert!(matches!(r, WorkRef::FilePath(_)));
    }
}
