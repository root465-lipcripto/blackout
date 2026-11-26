use std::fs;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub podman: Option<String>,
    pub rootless: Option<bool>,
    pub host_architecture: String,
    pub limitations: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct NetworkCheck {
    pub backend: &'static str,
    pub image: String,
    pub isolated: bool,
    pub probe: &'static str,
    pub details: String,
}

#[derive(Debug, Serialize)]
pub struct PreparedProject {
    pub project: String,
    pub path: PathBuf,
    pub packages: usize,
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub action: String,
    pub image: String,
    pub passed: bool,
    pub network_isolated: bool,
    pub elapsed_ms: u128,
    pub details: String,
}

#[derive(Debug, Serialize, serde::Deserialize)]
pub struct ValidationEvidence {
    pub action: String,
    pub image: String,
    pub passed: bool,
    pub network_isolated: bool,
    pub elapsed_ms: u128,
    pub recorded_at_unix: u64,
}

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub format_version: u32,
    pub blackout_version: String,
    pub project: String,
    pub status: &'static str,
    pub ecosystems: Vec<String>,
    pub prepared: bool,
    pub target_os: String,
    pub target_architecture: String,
    pub validation: Vec<ValidationEvidence>,
    pub limitations: Vec<&'static str>,
}

pub fn doctor() -> DoctorReport {
    let version = Command::new("podman")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let rootless = Command::new("podman")
        .args(["info", "--format", "{{.Host.Security.Rootless}}"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| match String::from_utf8_lossy(&output.stdout).trim() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        });
    DoctorReport {
        podman: version,
        rootless,
        host_architecture: std::env::consts::ARCH.to_owned(),
        limitations: vec![
            "Project build and test commands are not yet implemented.",
            "The sandbox check validates network isolation only; it does not claim a hardened build sandbox.",
        ],
    }
}

pub fn network_check() -> Result<NetworkCheck> {
    let executable = std::env::current_exe().context("cannot locate blackout executable for probe")?;
    let listener = TcpListener::bind("127.0.0.1:0").context("cannot start local isolation probe listener")?;
    let address = listener.local_addr()?;
    let image =
        std::env::var("BLACKOUT_PROBE_IMAGE").unwrap_or_else(|_| "registry.fedoraproject.org/fedora:latest".into());
    let mount = format!("{}:/blackout:ro", executable.display());
    let output = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--pull=never",
            "--network=none",
            "--read-only",
            "--cap-drop=all",
            "--security-opt=no-new-privileges",
            "--userns=keep-id",
            "--user=65534:65534",
            "--pids-limit=32",
            "--memory=256m",
            "--cpus=1",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=16m",
            "--volume",
        ])
        .arg(mount)
        .arg(&image)
        .arg("/blackout")
        .arg("__probe")
        .arg(address.to_string())
        .output()
        .context("Podman is unavailable or failed to start the network-none probe")?;
    drop(listener);
    let details = format!(
        "status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(NetworkCheck {
        backend: "podman",
        image,
        isolated: output.status.success(),
        probe: "container attempted TCP to a host-loopback listener and did not connect",
        details,
    })
}

pub fn prepare(source: &Path, destination: &Path) -> Result<PreparedProject> {
    let report = crate::project::scan(source)?;
    if !report.ecosystems.iter().any(|item| item.name == "cargo") {
        bail!("prepare currently supports Cargo projects only");
    }
    if !report.path.join("Cargo.lock").is_file() {
        bail!("Cargo.lock is required; generate and review a lockfile before preparing");
    }
    if report.path.join(".cargo/config.toml").exists() || report.path.join(".cargo/config").exists() {
        bail!(
            "a project-level .cargo/config.toml already exists; merge it manually with Cargo's vendor configuration before preparing"
        );
    }
    if destination.exists() {
        bail!("refusing to overwrite existing destination {}", destination.display());
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .with_context(|| {
            format!(
                "destination parent {} does not exist",
                destination
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .display()
            )
        })?;
    let dest = parent.join(
        destination
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("destination needs a directory name"))?,
    );
    if dest.starts_with(&report.path) || report.path.starts_with(&dest) {
        bail!("source and prepared destination must be separate directories");
    }
    fs::create_dir(&dest)?;
    let copy_result = (|| -> Result<()> {
        for entry in walkdir::WalkDir::new(&report.path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || (!entry.file_type().is_symlink()
                        && !crate::project::excluded_dir(entry.file_name().to_string_lossy().as_ref()))
            })
        {
            let entry = entry?;
            if entry.depth() == 0 {
                continue;
            }
            let relative = entry.path().strip_prefix(&report.path)?;
            let name = relative.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            if name == ".env"
                || name.starts_with(".env.")
                || name.ends_with(".pem")
                || name.ends_with(".key")
                || name.ends_with(".p12")
                || name.ends_with(".pfx")
                || name == "id_rsa"
                || name == "id_ed25519"
            {
                continue;
            }
            let target = dest.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(entry.path(), &target).with_context(|| format!("cannot copy {}", relative.display()))?;
            }
        }
        let manifest = dest.join("Cargo.toml");
        let mut command = Command::new("cargo");
        command
            .current_dir(&dest)
            .args(["vendor", "--locked", "--manifest-path"])
            .arg(&manifest)
            .arg("vendor")
            .env("CARGO_NET_GIT_FETCH_WITH_CLI", "false");
        let output = command.output().context("Cargo is required for preparation")?;
        if !output.status.success() {
            bail!(
                "cargo vendor failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let config = String::from_utf8(output.stdout).context("cargo vendor emitted invalid UTF-8 configuration")?;
        if !config.trim().is_empty() {
            let config_path = dest.join(".cargo/config.toml");
            fs::create_dir_all(config_path.parent().unwrap())?;
            fs::write(config_path, config)?;
        }
        Ok(())
    })();
    if let Err(error) = copy_result {
        let _ = fs::remove_dir_all(&dest);
        return Err(error);
    }
    let packages = fs::read_dir(dest.join("vendor"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    Ok(PreparedProject {
        project: report.name,
        path: dest,
        packages,
    })
}

pub fn validate(path: &Path, action: &str) -> Result<ValidationResult> {
    if !matches!(action, "build" | "test") {
        bail!("action must be build or test");
    }
    let root = path
        .canonicalize()
        .with_context(|| format!("prepared project {} does not exist", path.display()))?;
    if !root.join("Cargo.toml").is_file() || !root.join(".cargo/config.toml").is_file() || !root.join("vendor").is_dir()
    {
        bail!("project is not prepared; run `blackout prepare <source> --output <new-directory>` first");
    }
    if root.to_string_lossy().contains(':') {
        bail!("sandbox project path cannot contain ':'");
    }
    let image = std::env::var("BLACKOUT_SANDBOX_IMAGE").unwrap_or_else(|_| "rust:1.85-slim".into());
    let network = network_check()?;
    if !network.isolated {
        bail!(
            "network isolation could not be demonstrated; build/test was not started: {}",
            network.details
        );
    }
    let action_args: &[&str] = if action == "build" {
        &["build", "--offline", "--locked"]
    } else {
        &["test", "--offline", "--locked"]
    };
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .context("cannot determine current uid")?;
    let gid = Command::new("id")
        .arg("-g")
        .output()
        .context("cannot determine current gid")?;
    let user = format!(
        "{}:{}",
        String::from_utf8_lossy(&uid.stdout).trim(),
        String::from_utf8_lossy(&gid.stdout).trim()
    );
    let started = Instant::now();
    let mut child = Command::new("podman")
        .args([
            "run",
            "--rm",
            "--pull=never",
            "--network=none",
            "--read-only",
            "--cap-drop=all",
            "--security-opt=no-new-privileges",
            "--userns=keep-id",
            "--user",
        ])
        .arg(user)
        .args([
            "--pids-limit=256",
            "--memory=2g",
            "--cpus=2",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=256m",
            "--env",
            "CARGO_NET_OFFLINE=true",
            "--env",
            "CARGO_HOME=/tmp/cargo-home",
            "--env",
            "CARGO_TARGET_DIR=/workspace/target",
            "--volume",
        ])
        .arg(format!("{}:/workspace:rw", root.display()))
        .args(["--workdir=/workspace"])
        .arg(&image)
        .arg("cargo")
        .args(action_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Podman is unavailable; offline build/test was not run")?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_reader = thread::spawn(move || read_limited(stdout, 20_000));
    let stderr_reader = thread::spawn(move || read_limited(stderr, 20_000));
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(600) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            bail!("offline {action} exceeded the 600 second timeout; Podman was asked to stop the container");
        }
        thread::sleep(Duration::from_millis(100));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("stderr reader panicked"))??;
    let details = format!("status={status}; stdout={stdout}; stderr={stderr}");
    let result = ValidationResult {
        action: action.to_owned(),
        image,
        passed: status.success(),
        network_isolated: network.isolated,
        elapsed_ms: started.elapsed().as_millis(),
        details,
    };
    record_validation(path, &result)?;
    Ok(result)
}

fn record_validation(path: &Path, result: &ValidationResult) -> Result<()> {
    let evidence = ValidationEvidence {
        action: result.action.clone(),
        image: result.image.clone(),
        passed: result.passed,
        network_isolated: result.network_isolated,
        elapsed_ms: result.elapsed_ms,
        recorded_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };
    let state = path.join(".blackout");
    fs::create_dir_all(&state)?;
    let file = tempfile::NamedTempFile::new_in(&state)?;
    serde_json::to_writer_pretty(file.as_file(), &evidence)?;
    file.as_file().sync_all()?;
    let destination = state.join(format!("last-{}.json", result.action));
    file.persist(destination)?;
    Ok(())
}

pub fn report(path: &Path) -> Result<StatusReport> {
    let project = crate::project::scan(path)?;
    let prepared = project.ecosystems.iter().any(|ecosystem| ecosystem.name == "cargo")
        && project.path.join("Cargo.toml").is_file()
        && project.path.join(".cargo/config.toml").is_file()
        && project.path.join("vendor").is_dir();
    let mut validation = Vec::new();
    for action in ["build", "test"] {
        let file = project.path.join(".blackout").join(format!("last-{action}.json"));
        if file.is_file() {
            let metadata = fs::metadata(&file)?;
            if metadata.len() > 32 * 1024 {
                bail!("{} exceeds the report evidence size limit", file.display());
            }
            let record: ValidationEvidence = serde_json::from_slice(&fs::read(&file)?)
                .with_context(|| format!("invalid validation record at {}", file.display()))?;
            if record.action != action {
                bail!("unexpected action in validation record {}", file.display());
            }
            validation.push(record);
        }
    }
    let status = if project.ecosystems.is_empty() {
        "UNSUPPORTED"
    } else if validation
        .iter()
        .any(|record| !record.passed || !record.network_isolated)
    {
        "FAILED"
    } else if validation.len() == 2 && validation.iter().all(|record| record.passed && record.network_isolated) {
        "PARTIALLY_VERIFIED"
    } else if prepared {
        "READY"
    } else {
        "INCOMPLETE"
    };
    Ok(StatusReport {
        format_version: 1,
        blackout_version: env!("CARGO_PKG_VERSION").into(),
        project: project.name,
        status,
        ecosystems: project.ecosystems.into_iter().map(|ecosystem| ecosystem.name).collect(),
        prepared,
        target_os: std::env::consts::OS.into(),
        target_architecture: std::env::consts::ARCH.into(),
        validation,
        limitations: vec![
            "Validation records contain the last command result and are not bound to a source fingerprint.",
            "A container result is specific to the selected image and host runtime.",
        ],
    })
}

pub fn report_markdown(report: &StatusReport) -> String {
    let mut output = format!(
        "# BLACKOUT report: {}\n\n- Status: **{}**\n- BLACKOUT: {}\n- Target: {} / {}\n- Prepared Cargo copy: {}\n- Ecosystems: {}\n",
        report.project,
        report.status,
        report.blackout_version,
        report.target_os,
        report.target_architecture,
        report.prepared,
        if report.ecosystems.is_empty() {
            "none".to_owned()
        } else {
            report.ecosystems.join(", ")
        },
    );
    output.push_str("\n## Last validation results\n\n");
    if report.validation.is_empty() {
        output.push_str("No validation result has been recorded.\n");
    } else {
        for record in &report.validation {
            output.push_str(&format!(
                "- {}: passed={}, network_isolated={}, elapsed={} ms, image={}\n",
                record.action, record.passed, record.network_isolated, record.elapsed_ms, record.image
            ));
        }
    }
    output.push_str("\n## Limitations\n\n");
    for limitation in &report.limitations {
        output.push_str(&format!("- {limitation}\n"));
    }
    output
}

fn read_limited(mut input: impl Read, limit: usize) -> std::io::Result<String> {
    let mut kept = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok(String::from_utf8_lossy(&kept).into_owned())
}

pub fn probe(address: &str) -> i32 {
    let Ok(address) = address.parse() else {
        return 2;
    };
    match TcpStream::connect_timeout(&address, Duration::from_millis(800)) {
        Ok(stream) => {
            drop(stream);
            9
        }
        Err(_) => 0,
    }
}

#[allow(dead_code)]
pub fn executable_path() -> Result<PathBuf> {
    std::env::current_exe().context("cannot resolve current executable")
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use super::probe;

    #[test]
    fn probe_detects_a_listening_tcp_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let accept = thread::spawn(move || {
            let _ = listener.accept();
        });
        thread::sleep(Duration::from_millis(20));
        assert_eq!(probe(&address), 9);
        accept.join().unwrap();
    }

    #[test]
    fn probe_treats_a_closed_port_as_blocked() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        drop(listener);
        assert_eq!(probe(&address), 0);
    }
}
