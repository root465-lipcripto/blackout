use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "blackout", version, about = "Disconnect the world. Keep building.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(name = "__probe", hide = true)]
    Probe { address: String },
    /// Inspect a directory and report detected project ecosystems.
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Check local storage, tools, and an optional offline container runtime.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Copy a Cargo project and vendor its locked dependencies without editing the source tree.
    Prepare {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Run Cargo build or tests offline inside a rootless network-disabled Podman container.
    Validate {
        #[arg(value_parser = ["build", "test"])]
        action: String,
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Generate a machine-readable or Markdown status report.
    Report {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Manage the local SHA-256 content-addressed object cache.
    Cache {
        #[command(subcommand)]
        command: CacheCommand,
    },
    /// Create and inspect portable .blackout bundles.
    Bundle {
        #[command(subcommand)]
        command: BundleCommand,
    },
    /// Restore a verified bundle into a new directory.
    Restore { file: PathBuf, destination: PathBuf },
    /// Validate Podman's network-none isolation without running project code.
    SandboxCheck,
}

#[derive(Subcommand)]
enum CacheCommand {
    /// Store a file in the local object cache.
    Put { file: PathBuf },
    /// Verify an object by its lowercase SHA-256 digest.
    Verify { digest: String },
    /// Show cache size and object count.
    Status,
}

#[derive(Subcommand)]
enum BundleCommand {
    /// Create a bundle from a project directory.
    Create {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Verify a bundle's manifest and every contained file checksum.
    Verify { file: PathBuf },
    /// Display a bundle manifest after verification.
    Inspect { file: PathBuf },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("blackout: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Probe { address } => std::process::exit(blackout::sandbox::probe(&address)),
        Command::Scan { path, json } => {
            let report = blackout::project::scan(&path).context("could not inspect project")?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Project: {}", report.name);
                println!("Path: {}", report.path.display());
                println!("Status: {}", report.status);
                for ecosystem in report.ecosystems {
                    println!(
                        "- {}: {}",
                        ecosystem.name,
                        if ecosystem.supported {
                            "Cargo supported"
                        } else {
                            "detected only"
                        }
                    );
                }
            }
        }
        Command::Doctor { json } => {
            let report = blackout::sandbox::doctor();
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Podman: {}", report.podman.unwrap_or_else(|| "unavailable".into()));
                println!(
                    "Rootless: {}",
                    report
                        .rootless
                        .map_or("unknown", |value| if value { "yes" } else { "no" })
                );
                println!("Network-none check: available only with `blackout sandbox-check`");
            }
        }
        Command::Prepare { path, output } => {
            let result = blackout::sandbox::prepare(&path, &output)?;
            println!(
                "Prepared {} at {} ({} vendored packages)",
                result.project,
                result.path.display(),
                result.packages
            );
            println!("The source tree was not modified; Cargo build scripts were not run.");
        }
        Command::Validate { action, path } => {
            let result = blackout::sandbox::validate(&path, &action)?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !result.passed {
                bail!("offline {action} validation failed");
            }
        }
        Command::Report { path, json } => {
            let report = blackout::sandbox::report(&path)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", blackout::sandbox::report_markdown(&report));
            }
        }
        Command::Cache { command } => match command {
            CacheCommand::Put { file } => {
                let (digest, size, path) = blackout::cache::put(&file)?;
                println!("sha256:{digest}  {size} bytes  {}", path.display());
            }
            CacheCommand::Verify { digest } => {
                let path = blackout::cache::verify(&digest)?;
                println!("verified sha256:{digest} at {}", path.display());
            }
            CacheCommand::Status => {
                let report = blackout::cache::status()?;
                println!("Objects: {}", report.objects);
                println!("Bytes: {}", report.bytes);
                println!("Store: {}", report.root.display());
            }
        },
        Command::Bundle { command } => match command {
            BundleCommand::Create { path, output } => {
                let manifest = blackout::bundle::create(&path, &output)?;
                println!(
                    "Created {} ({} files, project {})",
                    output.display(),
                    manifest.files.len(),
                    manifest.project
                );
            }
            BundleCommand::Verify { file } => {
                let manifest = blackout::bundle::verify(&file)?;
                println!(
                    "verified {} ({} files, {})",
                    file.display(),
                    manifest.files.len(),
                    manifest.project
                );
            }
            BundleCommand::Inspect { file } => {
                let manifest = blackout::bundle::verify(&file)?;
                println!("{}", serde_json::to_string_pretty(&manifest)?);
            }
        },
        Command::Restore { file, destination } => {
            let manifest = blackout::bundle::restore(&file, &destination)?;
            println!(
                "restored {} ({} files) to {}",
                manifest.project,
                manifest.files.len(),
                destination.display()
            );
        }
        Command::SandboxCheck => {
            let status = blackout::sandbox::network_check()?;
            println!("{}", serde_json::to_string_pretty(&status)?);
            if !status.isolated {
                bail!("network isolation was not demonstrated");
            }
        }
    }
    Ok(())
}
