//! Frontend-manifest seam.
//!
//! A debug build without the frontend still compiles (the server runs
//! API-only and says so at `/`); a release build without it fails loudly
//! here — correctness by construction in both directions.

use std::env;
use std::fs;
use std::path::Path;

const MANIFEST: &str = "ui/dist/.vite/manifest.json";

fn main() {
    println!("cargo::rerun-if-changed={MANIFEST}");
    println!("cargo::rustc-check-cfg=cfg(ui_built)");
    let out_dir = env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    let dest = Path::new(&out_dir).join("vite-manifest.json");
    match fs::read_to_string(MANIFEST) {
        Ok(json) => {
            json.parse::<vite_chunks::Manifest>().expect("Vite manifest to be valid");
            fs::write(&dest, json).expect("write manifest to OUT_DIR");
            println!("cargo::rustc-cfg=ui_built");
        },
        Err(_) => {
            assert!(
                env::var("PROFILE").expect("cargo sets PROFILE") != "release",
                "frontend not built: run `make assets` first"
            );
            fs::write(&dest, "{}").expect("write empty manifest to OUT_DIR");
        },
    }
}
