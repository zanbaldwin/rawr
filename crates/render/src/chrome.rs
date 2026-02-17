use crate::error::{ErrorKind, Result};
use std::{path::PathBuf, process::Command};

/// Represents a Chrome/Chromium executable.
pub(crate) enum Chrome {
    /// A directly executable binary.
    Binary { path: PathBuf },
    /// A Flatpak-installed application.
    Flatpak { app_id: String },
}
impl Chrome {
    pub(crate) fn discover() -> Result<Self> {
        // Check for direct executables
        // TODO: What are the executable names on Windows? macOS?
        let executables = ["google-chrome", "chromium", "chromium-browser", "chrome"];
        for exe in executables {
            if let Ok(path) = which::which(exe) {
                return Ok(Self::Binary { path });
            }
        }
        tracing::info!("Chrome executable not found in PATH");
        if let Ok(flatpak) = which::which("flatpak") {
            tracing::trace!(flatpak = %flatpak.display(), "Discovered Flatpak on system; searching installed apps");
            // Check Flatpak installations
            let flatpak_apps = ["com.google.Chrome", "org.chromium.Chromium"];
            for app_id in flatpak_apps {
                if Command::new(&flatpak).args(["info", app_id]).output().is_ok_and(|o| o.status.success()) {
                    return Ok(Self::Flatpak { app_id: app_id.to_string() });
                }
            }
        } else {
            tracing::info!("Flatpak not found; skipping containerized Chrome checks.");
        }
        exn::bail!(ErrorKind::ChromeNotFound);
    }
}
