use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(about = "Serve local packages through a mock GitHub release API")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,

    #[arg(long, default_value = "http://127.0.0.1:8080")]
    public_url: String,

    #[arg(long, default_value = "LowderPlay/cheburcheck")]
    repository: String,

    /// Directory containing .deb, .apk, and .ipk release assets.
    #[arg(long)]
    assets_dir: PathBuf,
}

#[derive(Serialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    validate_repository(&args.repository)?;
    let assets_dir = args
        .assets_dir
        .canonicalize()
        .with_context(|| format!("failed to open {}", args.assets_dir.display()))?;
    let public_url = args.public_url.trim_end_matches('/').to_owned();
    let listener = TcpListener::bind(&args.bind)
        .with_context(|| format!("failed to listen on {}", args.bind))?;

    println!(
        "mock release API: {public_url}/repos/{}/releases/latest",
        args.repository
    );
    println!("serving assets from {}", assets_dir.display());

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                if let Err(error) =
                    handle_request(stream, &assets_dir, &args.repository, &public_url)
                {
                    eprintln!("request failed: {error:#}");
                }
            }
            Err(error) => eprintln!("failed to accept connection: {error}"),
        }
    }
    Ok(())
}

fn handle_request(
    mut stream: TcpStream,
    assets_dir: &Path,
    repository: &str,
    public_url: &str,
) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone().context("failed to read request")?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("failed to read request line")?;
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(path), Some(_version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return respond(&mut stream, 400, "text/plain", b"bad request\n");
    };
    if method != "GET" {
        return respond(&mut stream, 405, "text/plain", b"method not allowed\n");
    }
    println!("request: {method} {path}");

    let release_path = format!("/repos/{repository}/releases/latest");
    if path == release_path {
        let body = serde_json::to_vec(&Release {
            assets: list_assets(assets_dir, public_url)?,
        })?;
        return respond(&mut stream, 200, "application/json", &body);
    }

    if let Some(name) = path.strip_prefix("/assets/") {
        if !valid_name(name) {
            return respond(&mut stream, 400, "text/plain", b"invalid asset name\n");
        }
        let asset_path = assets_dir.join(name);
        return match fs::read(&asset_path) {
            Ok(body) => respond(&mut stream, 200, "application/octet-stream", &body),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                respond(&mut stream, 404, "text/plain", b"not found\n")
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to read {}", asset_path.display()))
            }
        };
    }

    respond(&mut stream, 404, "text/plain", b"not found\n")
}

fn list_assets(directory: &Path, public_url: &str) -> Result<Vec<Asset>> {
    let mut assets = Vec::new();
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to list {}", directory.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("asset filename is not valid UTF-8"))?;
        let supported_extension = matches!(
            entry.path().extension().and_then(|value| value.to_str()),
            Some("deb" | "apk" | "ipk")
        );
        if !valid_name(&name) || !supported_extension {
            continue;
        }
        assets.push(Asset {
            browser_download_url: format!("{public_url}/assets/{name}"),
            name,
        });
    }
    assets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(assets)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn validate_repository(repository: &str) -> Result<()> {
    let mut parts = repository.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) if valid_name(owner) && valid_name(repo) => Ok(()),
        _ => bail!("invalid repository {repository:?}; expected owner/name"),
    }
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_package_names_and_rejects_paths() {
        assert!(valid_name("cheburprobe-0.6.0-r1_x86_64.apk"));
        assert!(valid_name("luci-app-cheburprobe_0.6.0-1_all.ipk"));
        assert!(!valid_name("../cheburprobe.apk"));
        assert!(!valid_name("directory/cheburprobe.apk"));
    }
}
