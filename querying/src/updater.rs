use async_trait::async_trait;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use reqwest::IntoUrl;
use std::fmt::Display;
use std::io;
use std::io::Error;

pub async fn fetch_db<T: IntoUrl + Display>(url: T) -> Result<Vec<u8>, Error> {
    info!("Fetching {}", url);
    let response = reqwest::get(url).await
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?
        .error_for_status()
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?;

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .map_err(|e| Error::new(io::ErrorKind::Other, e))?
        .progress_chars("#>-"));

    let mut bytes = Vec::new();
    bytes.reserve(total_size as usize);
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| Error::new(io::ErrorKind::Other, e))?;
        bytes.extend(&chunk);
        pb.inc(chunk.len() as u64);
    }

    pb.finish_with_message("Download complete!");

    Ok(bytes)
}

#[async_trait]
pub trait Updatable {
    type Base;
    async fn download() -> Result<Self::Base, Error>;
    async fn install(&mut self, base: Self::Base) -> Result<(), Error>;
    fn get_url(key: &'static str, default: &'static str) -> String {
        std::env::var(key).ok().unwrap_or(default.to_string())
    }
}
