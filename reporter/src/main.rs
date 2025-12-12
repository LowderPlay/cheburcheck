mod resolver;
mod counter;

use crate::resolver::Resolver;
use anyhow::Result;
use clap::{Parser, ValueEnum};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indicatif::{ProgressIterator, ProgressStyle};
use log::{error, info, warn, LevelFilter};
use reports::{AgencyReport, Evidence, ReporterConfig};
use reqwest::redirect::Policy;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use counter::Counter;

const JUNK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/junk.bin"));

#[derive(Serialize, Debug, Ord, PartialOrd, Eq, PartialEq, Clone, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum Verbosity {
    Silent,
    Error,
    Block,
    All,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "DPI probe: checks blockage of domains by SNI")]
struct Args {
    /// Output results file
    #[arg(required = false)]
    output: Option<PathBuf>,

    /// Fake target to establish probing settings.
    /// Use with caution - it might trigger TLS block
    #[arg(short, long, required = false)]
    fake: Option<String>,

    /// Take first N targets
    #[arg(short, long, default_value_t = 100_000)]
    count: usize,

    /// Read timeout in seconds
    #[arg(short, long, default_value_t = 5)]
    timeout_secs: u64,

    /// Maximum concurrent probes. Make sure that it doesn't exceed 'ulimit -n'
    #[arg(short, long = "probes", default_value_t = 1000)]
    probe_count: usize,

    /// Display probing results in console
    #[arg(short, long, default_value_t = Verbosity::Silent, value_enum)]
    verbosity: Verbosity,

    /// Attempts to establish connection
    #[arg(short, long, default_value_t = 2)]
    retry_count: usize,

    /// Try using plain HTTP without TLS
    #[arg(short = 'H', long, default_value_t = false)]
    http: bool,

    /// Send 64kb junk to server
    #[arg(short = 'x', long, default_value_t = false)]
    tx: bool,

    /// Target IP to probe with.
    /// It should be included in IP-ranges of interest.
    /// The server must respond to any SNI/Host with a response larger than 64kb.
    #[arg(short, long, default_value = "5.78.7.195", value_parser = |v: &str| v.parse::<IpAddr>())]
    ip: IpAddr,

    /// File name on the server to test
    #[arg(short = 'P', long, default_value = "100MB.bin")]
    path: String,

    /// Custom agency endpoint address
    #[arg(short, long = "endpoint", default_value_t = option_env!("AGENCY_ENDPOINT")
                                            .unwrap_or("https://cheburcheck.ru/agency/report")
                                            .to_string())]
    agency_endpoint: String,

    /// Agency endpoint API key
    #[arg(short, long, env = "AGENCY_KEY")]
    key: Option<String>,

}

impl Args {
    fn to_reporter_config(&self) -> ReporterConfig {
        ReporterConfig {
            http: self.http,
            tx_junk: self.tx,
            ip: self.ip.clone(),
            path: self.path.clone(),
            retry_count: self.retry_count,
            timeout_secs: self.timeout_secs,
            probe_count: self.probe_count,
        }
    }
}

fn build_client(args: &Args, attempt: usize) -> reqwest::Result<Client> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .redirect(Policy::none())
        .use_rustls_tls()
        .dns_resolver(Arc::new(Resolver::new(args.ip)))
        .read_timeout(Duration::from_secs(args.timeout_secs * attempt as u64))
        .timeout(Duration::from_secs(15));

    Ok(client.build()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    env_logger::builder().filter_level(LevelFilter::Info).init();

    #[cfg(target_family = "unix")]
    {
        let file_limit: Option<usize> = unsafe { libc::getdtablesize() }.try_into().ok();
        if matches!(file_limit, Some(file_limit) if file_limit <= args.probe_count + 128) {
            warn!("Open file limit is too low ({})! Consider increasing it using `ulimit -n`.", file_limit.unwrap());
        }
    }

    let api_client = Client::new();
    info!("Loading targets list...");
    let targets = include_str!(concat!(env!("OUT_DIR"), "/list.csv"));
    let targets: Vec<String> = targets.lines().take(args.count)
        .map(|s| s.split(",").last().unwrap().to_string()).collect();

    info!("Probing {} domains with {} concurrent probes...", targets.len(), args.probe_count);
    let sem = Arc::new(tokio::sync::Semaphore::new(args.probe_count));
    let cancelled = wait_for_ctrlc();
    let start = Instant::now();
    let mut futs = FuturesUnordered::new();
    for target in targets.into_iter().progress()
        .with_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {human_pos}/{human_len} ({eta}, {per_sec})")?
            .progress_chars("#>-")) {
        if cancelled() {
            break;
        }
        let permit = sem.clone().acquire_owned().await?;
        let args = args.clone();
        let fake_target = args.fake.clone();
        futs.push(tokio::spawn(async move {
            let res = check_target(&args, fake_target.as_ref().unwrap_or(&target)).await;
            drop(permit);
            (target, res)
        }));
    }
    info!("Collecting results...");

    let mut counter = Counter::default();
    while let Some(res) = futs.next().await {
        match res {
            Ok((target, Ok(Verdict::Accepted))) => {
                counter.add(&target, Evidence::Ok);
            }
            Ok((target, Ok(Verdict::Blocked { early }))) => {
                counter.add(&target, Evidence::Blocked);
                if early {
                    counter.early += 1;
                }
            }
            Ok((target, Err(e))) if e.is_connect() => {
                counter.add(&target, Evidence::ConnectError);
                if args.verbosity >= Verbosity::Error {
                    println!("{e:?}");
                }
            }
            Ok((target, Err(_))) => {
                counter.add(&target, Evidence::Error);
            }
            Err(join_err) => {
                error!("Task join error: {}", join_err);
            }
        };
    }

    counter.print_results(&args.verbosity);
    if let Some(output) = &args.output {
        counter.save_results(output)?;
    }

    info!("Probed {} domains in {}s! \nSummary: {counter}", counter.total(), start.elapsed().as_secs());
    if let Err(e) = upload_results(&args, &api_client, counter.results).await {
        warn!("Upload failed: {}", e);
    }

    Ok(())
}

async fn upload_results(args: &Args, api_client: &Client, results: HashMap<String, Evidence>) -> Result<()> {
    info!("Uploading to {}", args.agency_endpoint);

    let uploaded = api_client.post(&args.agency_endpoint)
        .header("Content-Type", "application/msgpack")
        .body(rmp_serde::to_vec(&AgencyReport {
            version: env!("CARGO_PKG_VERSION").to_string(),
            config: args.to_reporter_config(),
            data: results,
        })?);

    let uploaded = if let Some(key) = &args.key {
        uploaded.header("Authorization", format!("Bearer {key}"))
    } else { uploaded };

    let uploaded = uploaded.send().await?;

    if uploaded.status().is_success() {
        info!("Uploaded ({})!", uploaded.status().to_string());
    } else {
        warn!("Upload failed: {}", uploaded.status().to_string());
    }
    info!("Agency response: {}", uploaded.text().await?);
    Ok(())
}

fn wait_for_ctrlc() -> impl Fn() -> bool {
    let cancelled = Arc::new(AtomicUsize::new(0));
    let cancelled_ctrlc = cancelled.clone();

    tokio::spawn(async move {
        loop {
            let _ = tokio::signal::ctrl_c().await;

            match cancelled_ctrlc.fetch_add(1, Ordering::SeqCst) {
                0 => warn!("Ctrl-C received. Finishing up and saving..."),
                _ => {
                    warn!("Forcing exit.");
                    std::process::exit(130);
                }
            }
        }
    });

    move || {
        cancelled.load(Ordering::SeqCst) != 0
    }
}

enum Verdict {
    Blocked { early: bool },
    Accepted,
}

async fn check_target(args: &Args, target: &str) -> Result<Verdict, reqwest::Error> {
    let url = format!("http{}://{target}/{}", if args.http {""} else {"s"}, args.path);
    let mut attempts = 0;

    loop {
        attempts += 1;
        let client = build_client(&args, 1)?;
        let mut resp = client.get(&url)
            .header("Range", "bytes=0-65536");
        if args.tx {
            resp = resp.body(JUNK)
        }
        let resp = resp.send()
            .await;

        let resp = match resp {
            Ok(resp) => match (resp.status(), resp.bytes().await) {
                (status, Ok(b)) => Ok((status, b)),
                (_, Err(e)) => Err((e, false)),
            },
            Err(e) => Err((e, true)),
        };
        return match resp {
            Ok((status, bytes)) => {
                let warn = if !status.is_success() {
                    Some(format!("Domain {target} returned non-OK code: {status}"))
                } else if bytes.len() < 65535 {
                    Some(format!("Domain {target} completed with {} bytes: \n{}", bytes.len(), String::from_utf8_lossy(bytes.as_ref())))
                } else {
                    None
                };

                if let Some(warn) = warn {
                    warn!("{warn}");
                    if attempts < args.retry_count {
                        continue;
                    } else {
                        return Ok(Verdict::Blocked { early: false });
                    }
                }

                Ok(Verdict::Accepted)
            }
            Err((e, early)) => {
                if attempts < args.retry_count {
                    continue;
                }
                if e.is_timeout() {
                    Ok(Verdict::Blocked { early })
                } else {
                    error!("{} -> Error: {:?}", target, e);
                    Err(e)
                }
            },
        }
    }
}
