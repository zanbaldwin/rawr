use crate::error::{ErrorKind, Result};
use exn::ResultExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use tracing::instrument;

/// Maximum time to wait for Chrome to finish rendering before killing.
const CHROME_TIMEOUT: Duration = Duration::from_secs(180);
/// How often to poll for process completion.
const CHROME_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Represents a Chrome/Chromium executable.
#[derive(Debug)]
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

    #[instrument]
    pub(crate) fn execute(&self, html: &Path, pdf: &Path) -> Result<()> {
        if !html.exists() || !pdf.is_absolute() || pdf.is_dir() {
            exn::bail!(ErrorKind::Io);
        }
        let mut cmd = match self {
            Self::Binary { path } => Command::new(path),
            Self::Flatpak { app_id } => {
                let mut c = Command::new("flatpak");
                c.args([
                    "run",
                    &format!("--filesystem={}", html.parent().unwrap().display()),
                    &format!("--filesystem={}", pdf.parent().unwrap().display()),
                    app_id,
                    "--",
                ]);
                c
            },
        };
        cmd.args([
            "--headless=new",
            "--disable-gpu",
            "--no-margins",
            "--run-all-compositor-stages-before-draw",
            "--font-render-hinting=none",
            "--no-pdf-header-footer",
            "--generate-pdf-document-outline",
            &format!("--print-to-pdf={}", pdf.display()),
            &format!("file://{}", html.display()),
        ]);
        let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().or_raise(|| ErrorKind::Io)?;
        let deadline = Instant::now() + CHROME_TIMEOUT;
        'child: loop {
            match child.try_wait().or_raise(|| ErrorKind::Io)? {
                Some(_) => break 'child,
                None if Instant::now() >= deadline => {
                    _ = child.kill();
                    _ = child.wait();
                    exn::bail!(ErrorKind::ChromeTimeout);
                },
                None => sleep(CHROME_POLL_INTERVAL),
            }
        }
        let output = child.wait_with_output().or_raise(|| ErrorKind::Io)?;
        if !output.status.success() {
            tracing::warn!(
                stdout = %String::from_utf8_lossy(&output.stdout),
                stderr = %String::from_utf8_lossy(&output.stderr),
                "Chrome rendering failed.",
            );
        }
        match output.status.code() {
            Some(0) => Ok(()),
            Some(c) => exn::bail!(ErrorKind::ChromeFailed(c)),
            None => exn::bail!(ErrorKind::ChromeTimeout),
        }
    }
}
