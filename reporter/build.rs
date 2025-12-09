use std::{env, fs, path::Path};
use reqwest::blocking::Client;

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let junk_path = Path::new(&out_dir).join("junk.bin");

    let mut data = vec![0u8; 64 * 1024];
    getrandom::fill(&mut data).expect("Failed to generate random bytes");
    fs::write(&junk_path, &data).expect("Failed to write random data");

    let client = Client::new();
    println!("cargo:rerun-if-env-changed=DIST_DOMAIN_COUNT");
    let data = client.get(
        format!("https://tranco-list.eu/download/2NPQ9/{}", option_env!("DIST_DOMAIN_COUNT")
            .map(|x| x.parse::<u32>().expect("DIST_DOMAIN_COUNT is not a number"))
            .unwrap_or(1_000_000)))
        .send().expect("Failed to send request")
        .bytes().expect("Failed to read bytes");

    let list_csv = Path::new(&out_dir).join("list.csv");
    fs::write(&list_csv, &data).expect("Failed to write list");

    println!("cargo:rerun-if-changed=build.rs");
}
