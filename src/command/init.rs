use crate::error::Result;
use clap::Args;
use directories::UserDirs;
use rawr_config::default_config_path;
use rawr_output::{Line, Loudness, Output, PALETTE, Piece, Pipe};
use std::fs::write as sync_write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Create a new configuration file
#[derive(Debug, Args)]
pub(crate) struct InitCommand {}

impl InitCommand {
    fn default_library_dir() -> Option<PathBuf> {
        UserDirs::new().and_then(|dirs| dirs.document_dir().map(|d| d.join("Books/AO3")))
    }

    fn generate_config(library_dir: &Path) -> String {
        format!("[targets.default]\ndriver = \"local\"\ndirectory = \"{}\"\n", library_dir.display())
    }

    pub async fn execute(&self, output: &dyn Output) -> Result<ExitCode> {
        let config_path =
            default_config_path().ok_or_else(|| miette::miette!("Could not determine platform config directory"))?;

        if config_path.exists() {
            output.print_to(
                Pipe::Err,
                &Line::new([
                    Piece::fixed("Configuration file already exists:", &PALETTE.danger),
                    Piece::space(),
                    Piece::plain(config_path.display().to_string()),
                ])
                .with_volume(Loudness::Shout),
            );
            output.print_to(
                Pipe::Err,
                &Line::new([Piece::fixed(
                    "Delete the existing file to generate a new one.",
                    &PALETTE.muted,
                )]),
            );
            return Ok(ExitCode::FAILURE);
        }

        let default_library = Self::default_library_dir();
        let library_dir = prompt_library_dir(default_library.as_deref())?;

        let toml_content = Self::generate_config(&library_dir);
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("Failed to create config directory '{}': {e}", parent.display()))?;
        }
        sync_write(&config_path, &toml_content)
            .map_err(|e| miette::miette!("Failed to write config file '{}': {e}", config_path.display()))?;

        output.print_to(Pipe::Out, &Line::empty());
        output.print_to(
            Pipe::Out,
            &Line::new([
                Piece::fixed("Config written to ", &PALETTE.success),
                Piece::plain(config_path.display().to_string()),
            ])
            .with_volume(Loudness::Shout),
        );
        output.print_to(
            Pipe::Out,
            &Line::new([
                Piece::fixed("Library directory: ", &PALETTE.success),
                Piece::plain(library_dir.display().to_string()),
            ])
            .with_volume(Loudness::Shout),
        );

        output.print_to(Pipe::Out, &Line::empty());
        output.print_to(
            Pipe::Out,
            &Line::new([Piece::fixed(
                "Run `rawr scan` or `rawr import` to get started.",
                &PALETTE.muted,
            )]),
        );

        Ok(ExitCode::SUCCESS)
    }
}

fn expand_tilde(raw: &str) -> PathBuf {
    if raw.starts_with("~/") {
        UserDirs::new().map(|u| u.home_dir().join(&raw[2..])).unwrap_or_else(|| PathBuf::from(raw))
    } else {
        PathBuf::from(raw)
    }
}

fn is_inside_downloads(path: &Path) -> bool {
    let Some(download_dir) = UserDirs::new().and_then(|dirs| dirs.download_dir().map(|p| p.to_path_buf())) else {
        return false;
    };
    let path_canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let download_canonical = download_dir.canonicalize().unwrap_or(download_dir);
    path_canonical.starts_with(&download_canonical)
}

fn prompt_library_dir(default: Option<&Path>) -> Result<PathBuf> {
    let mut input = dialoguer::Input::<String>::new().with_prompt("Library directory");
    if let Some(default) = default {
        input = input.default(default.display().to_string());
    }
    let raw = input
        .validate_with(|input: &String| -> std::result::Result<(), String> {
            let expanded = expand_tilde(input);
            if is_inside_downloads(&expanded) {
                Err("Library cannot be inside your Downloads folder (primary import location). Choose a different location.".into())
            } else {
                Ok(())
            }
        })
        .interact_text()
        .map_err(|e| miette::miette!("{e}"))?;
    Ok(expand_tilde(&raw))
}
