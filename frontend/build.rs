use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

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

    run_pnpm(&manifest_dir, &["install", "--frozen-lockfile"])?;
    run_pnpm(&manifest_dir, &["build"])?;

    Ok(())
}

fn run_pnpm(manifest_dir: &PathBuf, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(pnpm());
    command.args(args).current_dir(manifest_dir);

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
