use std::{
    env,
    fs::{File},
    io::{self, Cursor},
    path::Path,
};

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
    let out_dir = env::var("OUT_DIR")?;
    let lucide_js_path = Path::new(&out_dir).join("lucide.js");

    let client = Client::new();

    println!("cargo:rerun-if-env-changed=LUCIDE_VERSION");
    let metadata_url = format!("https://registry.npmjs.org/lucide/{}", option_env!("LUCIDE_VERSION").unwrap_or("latest"));

    println!("Fetching npm metadata from {}", metadata_url);
    let metadata: NpmMetadata = client.get(metadata_url).send()?.json()?;

    println!("Downloading tarball from {}", metadata.dist.tarball);
    let tarball_bytes = client.get(&metadata.dist.tarball).send()?.bytes()?;

    let tar = GzDecoder::new(Cursor::new(tarball_bytes));
    let mut archive = Archive::new(tar);

    let mut lucide_js_found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path == Path::new("package/dist/umd/lucide.min.js") {
            println!("Found lucide.min.js, extracting to {:?}", lucide_js_path);
            let mut out_file = File::create(&lucide_js_path)?;
            io::copy(&mut entry, &mut out_file)?;
            lucide_js_found = true;
            break;
        }
    }

    if !lucide_js_found {
        panic!("Could not find dist/lucide.min.js inside the tarball");
    }

    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
