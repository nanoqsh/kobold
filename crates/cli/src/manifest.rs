use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

use crate::report::{Error, ErrorExt, Report};

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

pub fn manifest() -> Report<Manifest> {
    let out = Command::new("cargo")
        .arg("read-manifest")
        .output()
        .message("failed to run cargo")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::message(format!(
            "failed to read cargo manifest\n{err}",
        )));
    }

    let manifest: CargoManifest =
        serde_json::from_slice(&out.stdout).message("failed to parse cargo manifest")?;

    let out = Command::new("cargo")
        .args([
            "metadata",
            "--format-version=1",
            "--filter-platform=wasm32-unknown-unknown",
            "--no-deps",
        ])
        .output()
        .message("failed to run cargo")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::message(format!(
            "failed to read cargo metadata\n{err}",
        )));
    }

    let metadata: Metadata =
        serde_json::from_slice(&out.stdout).message("failed to parse cargo metadata")?;

    Command::new("cargo")
        .args(["build", "--release", "--target=wasm32-unknown-unknown"])
        .spawn()
        .message("failed to run cargo")?
        .wait()
        .message("failed to build cargo crate")?;

    Ok(Manifest {
        crate_name: manifest.name,
        target: metadata.target_directory,
    })
}
