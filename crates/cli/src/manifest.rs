use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::report::{Report, ReportExt};

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

pub fn manifest() -> Result<Manifest, Report> {
    let out = Command::new("cargo")
        .arg("read-manifest")
        .output()
        .with_message("failed to run cargo")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Report::message(format!(
            "failed to read cargo manifest\n{err}",
        )));
    }

    let manifest: CargoManifest =
        serde_json::from_slice(&out.stdout).with_message("failed to parse cargo manifest")?;

    let out = Command::new("cargo")
        .args([
            "metadata",
            "--format-version=1",
            "--filter-platform=wasm32-unknown-unknown",
            "--no-deps",
        ])
        .output()
        .with_message("failed to run cargo")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Report::message(format!(
            "failed to read cargo metadata\n{err}",
        )));
    }

    let metadata: Metadata =
        serde_json::from_slice(&out.stdout).with_message("failed to parse cargo metadata")?;

    Command::new("cargo")
        .args(["build", "--release", "--target=wasm32-unknown-unknown"])
        .spawn()
        .with_message("failed to run cargo")?
        .wait()
        .with_message("failed to build cargo crate")?;

    Ok(Manifest {
        crate_name: manifest.name,
        target: metadata.target_directory,
    })
}
