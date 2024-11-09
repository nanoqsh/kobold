use std::path::PathBuf;
use std::process::Command;

use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct CargoManifest {
    name: String,
}

#[derive(Deserialize)]
struct Metadata {
    target_directory: PathBuf,
}

pub struct Manifest {
    pub crate_name: String,
    pub target: PathBuf,
}

pub fn manifest() -> Result<Manifest> {
    let out = Command::new("cargo").arg("read-manifest").output()?;
    let manifest: CargoManifest = serde_json::from_slice(&out.stdout)?;

    let out = Command::new("cargo")
        .args([
            "metadata",
            "--format-version=1",
            "--filter-platform=wasm32-unknown-unknown",
            "--no-deps",
        ])
        .output()?;

    let metadata: Metadata = serde_json::from_slice(&out.stdout)?;

    Command::new("cargo")
        .args(["build", "--release", "--target=wasm32-unknown-unknown"])
        .spawn()?
        .wait()?;

    Ok(Manifest {
        crate_name: manifest.name,
        target: metadata.target_directory,
    })
}
