use anyhow::{Context, Result, bail};
use reqwest::{Client, Url};
use semver::Version;
use serde::Deserialize;
use std::env;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
use tempfile::TempDir;

const DEFAULT_REPOSITORY: &str = "LowderPlay/cheburcheck";
const DEFAULT_API_BASE_URL: &str = "https://api.github.com";
const LOCK_PATH: &str = "/tmp/cheburprobe-update.lock";

#[derive(Debug, Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackageKind {
    Debian,
    Apk,
    Opkg,
}

struct UpdateLock {
    _file: File,
}

impl UpdateLock {
    fn acquire() -> Result<Option<Self>> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(LOCK_PATH)
            .with_context(|| {
                format!("failed to open update lock {LOCK_PATH}; package updates must run as root")
            })?;

        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(rustix::io::Errno::WOULDBLOCK) => Ok(None),
            Err(error) => Err(error).context("failed to lock updater"),
        }
    }
}

pub async fn run() -> Result<()> {
    let Some(_lock) = UpdateLock::acquire()? else {
        println!("another cheburprobe update check is already running");
        return Ok(());
    };

    update().await
}

async fn update() -> Result<()> {
    let repository =
        env::var("CHEBURPROBE_UPDATE_REPOSITORY").unwrap_or_else(|_| DEFAULT_REPOSITORY.to_owned());
    validate_repository(&repository)?;

    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .context("the installed cheburprobe version is invalid")?;
    let client = Client::builder()
        .user_agent(concat!("cheburprobe-update/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()
        .context("failed to create HTTP client")?;
    let api_base_url = env::var("CHEBURPROBE_UPDATE_API_BASE_URL")
        .unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_owned());
    let api_url = format!(
        "{}/repos/{repository}/releases/latest",
        api_base_url.trim_end_matches('/')
    );
    let release = fetch_release(&client, &api_url).await?;
    let (kind, architecture, luci_installed) = detect_platform()?;
    let (asset, latest) = select_asset(&release.assets, kind, &architecture)?;

    if latest <= current {
        if latest == current {
            println!("cheburprobe is current ({current})");
        } else {
            println!(
                "installed cheburprobe {current} is newer than packaged version {latest}; not downgrading"
            );
        }
        return Ok(());
    }

    let luci_asset = luci_installed
        .then(|| select_luci_asset(&release.assets, kind, &latest))
        .transpose()?;

    let temp_dir = TempDir::with_prefix("cheburprobe-update.")
        .context("failed to create a temporary update directory")?;
    let package = download_asset(&client, asset, temp_dir.path()).await?;
    let luci_package = match luci_asset {
        Some(asset) => Some(download_asset(&client, asset, temp_dir.path()).await?),
        None => None,
    };
    install(kind, &package, luci_package.as_deref())?;

    println!("updated cheburprobe from {current} to {latest}");
    Ok(())
}

async fn fetch_release(client: &Client, api_url: &str) -> Result<Release> {
    client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to query the latest GitHub release")?
        .error_for_status()
        .context("GitHub rejected the latest-release request")?
        .json::<Release>()
        .await
        .context("GitHub returned an invalid release document")
}

fn validate_repository(repository: &str) -> Result<()> {
    let mut parts = repository.split('/');
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) if valid_part(owner) && valid_part(repo) => Ok(()),
        _ => bail!("invalid GitHub repository {repository:?}; expected owner/name"),
    }
}

fn command_exists(command: &str) -> bool {
    env::var_os("PATH").is_some_and(|path| {
        env::split_paths(&path).any(|directory| directory.join(command).is_file())
    })
}

fn command_output(command: &str, arguments: &[&str]) -> Result<Output> {
    let output = Command::new(command)
        .args(arguments)
        .output()
        .with_context(|| format!("failed to execute {command}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("{command} exited with {}: {stderr}", output.status);
    }
    Ok(output)
}

fn output_text(command: &str, arguments: &[&str]) -> Result<String> {
    let output = command_output(command, arguments)?;
    String::from_utf8(output.stdout).with_context(|| format!("{command} returned non-UTF-8 output"))
}

fn command_succeeds(command: &str, arguments: &[&str]) -> Result<bool> {
    let status = Command::new(command)
        .args(arguments)
        .status()
        .with_context(|| format!("failed to execute {command}"))?;
    Ok(status.success())
}

fn detect_platform() -> Result<(PackageKind, String, bool)> {
    if command_exists("dpkg") {
        let architecture = output_text("dpkg", &["--print-architecture"])?;
        let architecture = architecture.trim();
        if !matches!(architecture, "amd64" | "arm64") {
            bail!("unsupported Debian architecture: {architecture}");
        }
        Ok((PackageKind::Debian, architecture.to_owned(), false))
    } else if command_exists("apk") {
        let architecture = output_text("apk", &["--print-arch"])?;
        let luci_installed =
            command_succeeds("apk", &["info", "--exists", "luci-app-cheburprobe"])?;
        Ok((
            PackageKind::Apk,
            architecture.trim().to_owned(),
            luci_installed,
        ))
    } else if command_exists("opkg") {
        let architectures = output_text("opkg", &["print-architecture"])?;
        let architecture = architectures
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .rfind(|architecture| *architecture != "all")
            .context("opkg did not report a package architecture")?;
        let luci_installed = output_text("opkg", &["list-installed", "luci-app-cheburprobe"])?
            .lines()
            .any(|line| line.split_whitespace().next() == Some("luci-app-cheburprobe"));
        Ok((PackageKind::Opkg, architecture.to_owned(), luci_installed))
    } else {
        bail!("no supported package manager found");
    }
}

fn select_luci_asset<'a>(
    assets: &'a [Asset],
    kind: PackageKind,
    version: &Version,
) -> Result<&'a Asset> {
    let (prefix, suffix) = match kind {
        PackageKind::Apk => (format!("luci-app-cheburprobe-{version}-r"), ".apk"),
        PackageKind::Opkg => (format!("luci-app-cheburprobe_{version}-"), "_all.ipk"),
        PackageKind::Debian => bail!("LuCI packages are not supported on Debian"),
    };
    let matches: Vec<_> = assets
        .iter()
        .filter(|asset| asset.name.starts_with(&prefix) && asset.name.ends_with(suffix))
        .collect();
    match matches.as_slice() {
        [asset] => Ok(asset),
        [] => bail!("LuCI package for v{version} not found"),
        _ => bail!("multiple LuCI packages for v{version} found"),
    }
}

fn package_version(name: &str, kind: PackageKind, architecture: &str) -> Option<Version> {
    let (prefix, suffix) = match kind {
        PackageKind::Debian => ("cheburprobe_", format!("_{architecture}.deb")),
        PackageKind::Apk => ("cheburprobe-", format!("_{architecture}.apk")),
        PackageKind::Opkg => ("cheburprobe_", format!("_{architecture}.ipk")),
    };
    let version_with_revision = name.strip_prefix(prefix)?.strip_suffix(&suffix)?;
    let version = match kind {
        PackageKind::Apk => version_with_revision.rsplit_once("-r")?.0,
        PackageKind::Debian | PackageKind::Opkg => version_with_revision.rsplit_once('-')?.0,
    };
    Version::parse(version).ok()
}

fn select_asset<'a>(
    assets: &'a [Asset],
    kind: PackageKind,
    architecture: &str,
) -> Result<(&'a Asset, Version)> {
    let mut matches: Vec<_> = assets
        .iter()
        .filter_map(|asset| {
            package_version(&asset.name, kind, architecture).map(|version| (asset, version))
        })
        .collect();
    matches.sort_by(|(_, left), (_, right)| left.cmp(right));
    let Some((asset, version)) = matches.pop() else {
        bail!("Cheburprobe package for architecture {architecture} not found");
    };
    if matches.last().is_some_and(|(_, other)| other == &version) {
        bail!("multiple Cheburprobe {version} packages for architecture {architecture} found");
    }
    Ok((asset, version))
}

async fn download(client: &Client, url: Url, destination: &Path) -> Result<()> {
    let bytes = client
        .get(url)
        .send()
        .await
        .context("failed to download the update package")?
        .error_for_status()
        .context("GitHub rejected the package download")?
        .bytes()
        .await
        .context("failed to read the update package")?;
    std::fs::write(destination, bytes)
        .with_context(|| format!("failed to write {}", destination.display()))
}

async fn download_asset(
    client: &Client,
    asset: &Asset,
    directory: &Path,
) -> Result<std::path::PathBuf> {
    if Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str())
        != Some(&asset.name)
    {
        bail!("invalid release asset name: {:?}", asset.name);
    }
    let url = Url::parse(&asset.browser_download_url)
        .context("GitHub returned an invalid release asset URL")?;
    let destination = directory.join(&asset.name);
    download(client, url, &destination).await?;
    Ok(destination)
}

fn run_paths(command: &str, arguments: &[&Path]) -> Result<()> {
    let status = Command::new(command)
        .args(arguments)
        .status()
        .with_context(|| format!("failed to execute {command}"))?;
    if !status.success() {
        bail!("{command} exited with {status}");
    }
    Ok(())
}

fn run_args(command: &str, arguments: &[&str]) -> Result<()> {
    let status = Command::new(command)
        .args(arguments)
        .status()
        .with_context(|| format!("failed to execute {command}"))?;
    if !status.success() {
        bail!("{command} exited with {status}");
    }
    Ok(())
}

fn install(kind: PackageKind, package: &Path, luci_package: Option<&Path>) -> Result<()> {
    match kind {
        PackageKind::Debian => {
            run_paths("dpkg-deb", &[Path::new("--info"), package])?;
            run_paths("dpkg", &[Path::new("-i"), package])?;
            run_args("systemctl", &["try-restart", "cheburprobe.service"])
        }
        PackageKind::Apk => {
            let mut arguments = vec![Path::new("add"), Path::new("--allow-untrusted"), package];
            arguments.extend(luci_package);
            run_paths("apk", &arguments)?;
            run_args("/etc/init.d/cheburprobe", &["restart"])
        }
        PackageKind::Opkg => {
            let mut arguments = vec![Path::new("install"), package];
            arguments.extend(luci_package);
            run_paths("opkg", &arguments)?;
            run_args("/etc/init.d/cheburprobe", &["restart"])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_owned(),
            browser_download_url: format!(
                "https://github.com/LowderPlay/cheburcheck/releases/download/v0.5.0/{name}"
            ),
        }
    }

    #[test]
    fn parses_versions_from_package_names() {
        assert_eq!(
            package_version(
                "cheburprobe_1.2.3-1_arm64.deb",
                PackageKind::Debian,
                "arm64"
            ),
            Some(Version::new(1, 2, 3))
        );
        assert_eq!(
            package_version(
                "cheburprobe-1.2.3-r1_aarch64_generic.apk",
                PackageKind::Apk,
                "aarch64_generic"
            ),
            Some(Version::new(1, 2, 3))
        );
        assert_eq!(
            package_version(
                "cheburprobe_1.2.3-1_aarch64_generic.ipk",
                PackageKind::Opkg,
                "aarch64_generic"
            ),
            Some(Version::new(1, 2, 3))
        );
    }

    #[test]
    fn validates_repository_names() {
        assert!(validate_repository("LowderPlay/cheburcheck").is_ok());
        assert!(validate_repository("owner/repo/extra").is_err());
        assert!(validate_repository("owner?x/repo").is_err());
    }

    #[test]
    fn selects_each_package_format() {
        let assets = vec![
            asset("cheburprobe_0.5.0-1_arm64.deb"),
            asset("cheburprobe-0.5.0-r1_aarch64_generic.apk"),
            asset("cheburprobe_0.5.0-1_aarch64_generic.ipk"),
            asset("luci-app-cheburprobe-0.5.0-r1.apk"),
            asset("luci-app-cheburprobe_0.5.0-1_all.ipk"),
        ];
        let version = Version::new(0, 5, 0);
        assert_eq!(
            select_asset(&assets, PackageKind::Debian, "arm64")
                .unwrap()
                .0
                .name,
            "cheburprobe_0.5.0-1_arm64.deb"
        );
        assert_eq!(
            select_asset(&assets, PackageKind::Apk, "aarch64_generic")
                .unwrap()
                .0
                .name,
            "cheburprobe-0.5.0-r1_aarch64_generic.apk"
        );
        assert_eq!(
            select_asset(&assets, PackageKind::Opkg, "aarch64_generic")
                .unwrap()
                .0
                .name,
            "cheburprobe_0.5.0-1_aarch64_generic.ipk"
        );
        assert_eq!(
            select_luci_asset(&assets, PackageKind::Apk, &version)
                .unwrap()
                .name,
            "luci-app-cheburprobe-0.5.0-r1.apk"
        );
        assert_eq!(
            select_luci_asset(&assets, PackageKind::Opkg, &version)
                .unwrap()
                .name,
            "luci-app-cheburprobe_0.5.0-1_all.ipk"
        );
    }
}
