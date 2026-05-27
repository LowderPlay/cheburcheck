use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

#[derive(Deserialize)]
struct CargoManifest {
    package: CargoPackage,
}

#[derive(Deserialize)]
struct CargoPackage {
    version: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=pnpm-lock.yaml");
    println!("cargo:rerun-if-changed=svelte.config.js");
    println!("cargo:rerun-if-changed=vite.config.ts");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=static");
    println!("cargo:rerun-if-changed=../website/Cargo.toml");

    if std::env::var_os("SKIP_FRONTEND_BUILD").is_some() {
        return Ok(());
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let app_version = website_version(&manifest_dir)?;

    run_pnpm(&manifest_dir, &["install", "--frozen-lockfile"], None)?;
    run_pnpm(&manifest_dir, &["build"], Some(&app_version))?;

    Ok(())
}

fn website_version(manifest_dir: &PathBuf) -> Result<String, Box<dyn Error>> {
    let manifest = fs::read_to_string(manifest_dir.join("../website/Cargo.toml"))?;
    let manifest: CargoManifest = toml::from_str(&manifest)?;

    Ok(manifest.package.version)
}

fn run_pnpm(
    manifest_dir: &PathBuf,
    args: &[&str],
    app_version: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(pnpm());
    command.args(args).current_dir(manifest_dir);

    if let Some(app_version) = app_version {
        command.env("VITE_APP_VERSION", app_version);
    }

    let status = command.status()?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("pnpm {} failed", args.join(" ")).into())
    }
}

fn pnpm() -> &'static str {
    if cfg!(windows) { "pnpm.cmd" } else { "pnpm" }
}
