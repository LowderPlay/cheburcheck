use std::{
    env,
    fs::File,
    io::{self, Cursor},
    path::Path,
};
use std::collections::HashMap;
use std::path::PathBuf;
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde::Deserialize;
use tar::Archive;

#[derive(Deserialize)]
struct NpmMetadata {
    dist: Dist,
}

#[derive(Deserialize)]
struct Dist {
    tarball: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=migrations");
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    println!("cargo:rerun-if-env-changed=LUCIDE_VERSION");
    install_npm("lucide", option_env!("LUCIDE_VERSION"), HashMap::from([
        (PathBuf::from("package/dist/umd/lucide.min.js"), out_dir.join("lucide.js")),
    ]))?;

    println!("cargo:rerun-if-env-changed=CHARTJS_VERSION");
    install_npm("chart.js", option_env!("CHARTJS_VERSION"), HashMap::from([
        (PathBuf::from("package/dist/chart.umd.min.js"), out_dir.join("chart.js")),
    ]))?;

    println!("cargo:rerun-if-env-changed=CHARTDATALABELS_VERSION");
    install_npm("chartjs-plugin-datalabels", option_env!("CHARTDATALABELS_VERSION"), HashMap::from([
        (PathBuf::from("package/dist/chartjs-plugin-datalabels.min.js"), out_dir.join("chartjs-plugin-datalabels.js")),
    ]))?;

    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

fn install_npm(package: &str, version: Option<&str>, mut entries: HashMap<PathBuf, PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let metadata_url = format!(
        "https://registry.npmjs.org/{package}/{}",
        version.unwrap_or("latest")
    );

    println!("Fetching npm metadata from {}", metadata_url);
    let metadata: NpmMetadata = client.get(metadata_url).send()?.json()?;

    println!("Downloading tarball from {}", metadata.dist.tarball);
    let tarball_bytes = client.get(&metadata.dist.tarball).send()?.bytes()?;

    let tar = GzDecoder::new(Cursor::new(tarball_bytes));
    let mut archive = Archive::new(tar);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = PathBuf::from(entry.path()?);
        if let Some(dest) = entries.remove(&path) {
            println!("Found {:?}, extracting to {:?}", path.file_name(), dest);
            let mut out_file = File::create(&dest)?;
            io::copy(&mut entry, &mut out_file)?;
        }
    }

    if !entries.is_empty() {
        panic!("Not all entries found! {:?}", entries);
    }

    Ok(())
}
