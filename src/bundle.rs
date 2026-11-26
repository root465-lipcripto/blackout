use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::project::{excluded_dir, scan};

const MANIFEST: &str = "BLACKOUT-MANIFEST.json";
const MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 100_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    pub project: String,
    pub ecosystem: String,
    pub target: Target,
    pub files: Vec<FileRecord>,
    pub validation: Validation,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Target {
    pub os: String,
    pub architecture: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub mode: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Validation {
    pub network_isolated: Option<bool>,
    pub build_passed: Option<bool>,
    pub tests_passed: Option<bool>,
}

fn file_digest(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| anyhow::anyhow!("file size overflow"))?;
        if size > MAX_FILE_BYTES {
            bail!("{} exceeds the 1 GiB bundle file limit", path.display());
        }
        hash.update(&buffer[..count]);
    }
    Ok((format!("{:x}", hash.finalize()), size))
}

fn excluded_file(name: &str, relative: &Path) -> bool {
    if name == MANIFEST {
        return true;
    }
    if name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
    {
        return true;
    }
    if matches!(
        name,
        "id_rsa" | "id_ed25519" | "credentials" | "credentials.toml" | "config.json"
    ) && relative.components().any(|part| part.as_os_str() == ".cargo")
    {
        return true;
    }
    false
}

pub fn create(path: &Path, output: &Path) -> Result<BundleManifest> {
    let report = scan(path)?;
    let root = report.path;
    let output_abs = if output.exists() {
        output.canonicalize()?
    } else {
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()?;
        parent.join(
            output
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("bundle output needs a filename"))?,
        )
    };
    if output_abs.starts_with(&root) {
        bail!("bundle output must be outside the source project directory");
    }
    if output_abs.exists() {
        bail!("refusing to overwrite existing bundle {}", output_abs.display());
    }

    let mut records = Vec::new();
    let mut paths = Vec::new();
    let mut total = 0_u64;
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || (!entry.file_type().is_symlink() && !excluded_dir(entry.file_name().to_string_lossy().as_ref()))
        })
    {
        let entry = entry?;
        if entry.depth() == 0 || entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry.path().strip_prefix(&root)?.to_path_buf();
        if relative.to_str().is_none() {
            bail!("bundle paths must be valid UTF-8: {}", relative.display());
        }
        let name = relative.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if excluded_file(name, &relative) {
            continue;
        }
        if records.len() >= MAX_FILES {
            bail!("project exceeds the {MAX_FILES} file bundle limit");
        }
        let (sha256, size) = file_digest(entry.path())?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            entry.metadata()?.permissions().mode() & 0o777
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        total = total
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("bundle size overflow"))?;
        if total > MAX_BUNDLE_BYTES {
            bail!("project exceeds the 8 GiB uncompressed bundle limit");
        }
        records.push(FileRecord {
            path: relative.to_string_lossy().into_owned(),
            sha256,
            size,
            mode,
        });
        paths.push((entry.path().to_path_buf(), relative));
    }
    records.sort_by(|a, b| a.path.cmp(&b.path));
    paths.sort_by(|a, b| a.1.cmp(&b.1));
    let ecosystem = report
        .ecosystems
        .iter()
        .map(|item| item.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let manifest = BundleManifest {
        format_version: 1,
        project: report.name,
        ecosystem: if ecosystem.is_empty() {
            "unknown".into()
        } else {
            ecosystem
        },
        target: Target {
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        },
        files: records,
        validation: Validation {
            network_isolated: None,
            build_passed: None,
            tests_passed: None,
        },
    };

    let output_parent = output_abs.parent().expect("canonical output has parent");
    fs::create_dir_all(output_parent)?;
    let temporary = output_parent.join(format!(
        ".blackout-{}-{}.tmp",
        std::process::id(),
        output_abs.file_name().unwrap().to_string_lossy()
    ));
    let result = write_bundle(&temporary, &paths, &manifest).and_then(|()| {
        OpenOptions::new().write(true).open(&temporary)?.sync_all()?;
        fs::hard_link(&temporary, &output_abs)
            .with_context(|| format!("cannot commit bundle to {}", output_abs.display()))?;
        fs::remove_file(&temporary)?;
        Ok(())
    });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(manifest)
}

fn write_bundle(output: &Path, paths: &[(PathBuf, PathBuf)], manifest: &BundleManifest) -> Result<()> {
    let out = OpenOptions::new().write(true).create_new(true).open(output)?;
    let encoder = zstd::Encoder::new(out, 3)?;
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    for (source, relative) in paths {
        archive.append_path_with_name(source, relative)?;
    }
    let json = serde_json::to_vec_pretty(manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(json.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append_data(&mut header, MANIFEST, json.as_slice())?;
    archive.finish()?;
    let encoder = archive.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn safe_relative(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    !text.is_empty()
        && !text.contains('\\')
        && text
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path.components().all(|part| matches!(part, Component::Normal(_)))
}

fn read_and_validate(bundle: &Path, stage: Option<&Path>) -> Result<BundleManifest> {
    let input = File::open(bundle).with_context(|| format!("cannot open {}", bundle.display()))?;
    let decoder = zstd::Decoder::new(input).context("not a valid Zstandard stream")?;
    let mut archive = tar::Archive::new(decoder);
    let mut manifest = None;
    let mut files: BTreeMap<PathBuf, (String, u64)> = BTreeMap::new();
    let mut total = 0_u64;
    let mut entries_seen = 0_usize;
    for entry in archive.entries().context("invalid tar archive")? {
        entries_seen += 1;
        if entries_seen > MAX_FILES.saturating_mul(2).saturating_add(1) {
            bail!("bundle has too many archive entries");
        }
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !safe_relative(&path) {
            bail!("unsafe archive path: {}", path.display());
        }
        if path == Path::new(MANIFEST) {
            if manifest.is_some() {
                bail!("duplicate bundle manifest");
            }
            if !entry.header().entry_type().is_file() {
                bail!("bundle manifest is not a regular file");
            }
            if entry.header().size()? > 1024 * 1024 {
                bail!("bundle manifest exceeds 1 MiB");
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            manifest = Some(serde_json::from_slice::<BundleManifest>(&bytes).context("invalid bundle manifest JSON")?);
            continue;
        }
        if entry.header().entry_type().is_dir() {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            bail!("unsupported archive entry type for {}", path.display());
        }
        if files.len() >= MAX_FILES {
            bail!("bundle has more than {MAX_FILES} files");
        }
        if files.contains_key(&path) {
            bail!("duplicate archive path: {}", path.display());
        }
        let declared = entry.header().size()?;
        if declared > MAX_FILE_BYTES {
            bail!("{} exceeds the 1 GiB per-file limit", path.display());
        }
        total = total
            .checked_add(declared)
            .ok_or_else(|| anyhow::anyhow!("bundle size overflow"))?;
        if total > MAX_BUNDLE_BYTES {
            bail!("bundle exceeds the 8 GiB uncompressed limit");
        }
        let output = if let Some(stage) = stage {
            let output_path = stage.join(&path);
            fs::create_dir_all(output_path.parent().expect("safe relative file has parent"))?;
            Some(OpenOptions::new().write(true).create_new(true).open(output_path)?)
        } else {
            None
        };
        let mut output = output;
        let mut hash = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = entry.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            size = size
                .checked_add(count as u64)
                .ok_or_else(|| anyhow::anyhow!("file size overflow"))?;
            if size > declared || size > MAX_FILE_BYTES {
                bail!("archive entry exceeds its declared size: {}", path.display());
            }
            hash.update(&buffer[..count]);
            if let Some(file) = output.as_mut() {
                file.write_all(&buffer[..count])?;
            }
        }
        if size != declared {
            bail!("truncated archive entry: {}", path.display());
        }
        if let Some(file) = output.as_mut() {
            file.sync_all()?;
        }
        files.insert(path, (format!("{:x}", hash.finalize()), size));
    }
    let manifest = manifest.ok_or_else(|| anyhow::anyhow!("bundle manifest is missing"))?;
    if manifest.format_version != 1 {
        bail!("unsupported bundle format version {}", manifest.format_version);
    }
    if manifest.files.len() > MAX_FILES {
        bail!("manifest exceeds the file-count limit");
    }
    let mut expected = BTreeSet::new();
    for record in &manifest.files {
        let path = PathBuf::from(&record.path);
        if !safe_relative(&path) || record.path == MANIFEST {
            bail!("unsafe manifest path: {}", record.path);
        }
        if record.mode & !0o777 != 0 {
            bail!("unsafe permission bits for {}", record.path);
        }
        if !expected.insert(path.clone()) {
            bail!("duplicate manifest path: {}", record.path);
        }
        let (digest, size) = files
            .get(&path)
            .ok_or_else(|| anyhow::anyhow!("manifest file missing from archive: {}", record.path))?;
        if size != &record.size {
            bail!("size mismatch for {}", record.path);
        }
        if digest != &record.sha256 {
            bail!("checksum mismatch for {}", record.path);
        }
    }
    if files.keys().any(|path| !expected.contains(path)) {
        bail!("archive contains files absent from its manifest");
    }
    if let Some(stage) = stage {
        for record in &manifest.files {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let path = stage.join(&record.path);
                fs::set_permissions(path, fs::Permissions::from_mode((record.mode & 0o755) | 0o600))?;
            }
        }
    }
    Ok(manifest)
}

pub fn verify(bundle: &Path) -> Result<BundleManifest> {
    read_and_validate(bundle, None)
}

pub fn restore(bundle: &Path, destination: &Path) -> Result<BundleManifest> {
    if destination.exists() {
        bail!("restore destination {} must not already exist", destination.display());
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()?;
    let name = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("restore destination needs a directory name"))?;
    let root = parent.join(name);
    let stage = tempfile::Builder::new()
        .prefix(".blackout-restore-")
        .tempdir_in(&parent)?;
    let manifest = read_and_validate(bundle, Some(stage.path()))?;
    fs::create_dir(&root).with_context(|| {
        format!(
            "restore destination {} already exists or cannot be created",
            root.display()
        )
    })?;
    let result = (|| -> Result<()> {
        for record in &manifest.files {
            let relative = PathBuf::from(&record.path);
            if !safe_relative(&relative) {
                bail!("unsafe restore path: {}", relative.display());
            }
            let path = root.join(relative);
            let parent = path.parent().expect("validated relative path has a parent");
            fs::create_dir_all(parent)?;
            if !parent.canonicalize()?.starts_with(&root) {
                bail!("restore path escaped destination");
            }
            fs::rename(stage.path().join(&record.path), &path)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&root);
        return Err(error);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;

    use sha2::{Digest, Sha256};

    use tempfile::tempdir;

    use super::{BundleManifest, FileRecord, Target, Validation, create, restore, verify};

    #[test]
    fn round_trip_restores_only_source_files_and_omits_secrets() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("sample");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(project.join(".env"), "secret=never-bundle\n").unwrap();
        fs::create_dir(project.join("target")).unwrap();
        fs::write(project.join("target/noise"), "ignored\n").unwrap();
        let bundle = temp.path().join("sample.blackout");
        let manifest = create(&project, &bundle).unwrap();
        assert_eq!(manifest.files.len(), 2);
        verify(&bundle).unwrap();
        let output = temp.path().join("restored");
        restore(&bundle, &output).unwrap();
        assert_eq!(fs::read(output.join("main.rs")).unwrap(), b"fn main() {}\n");
        assert!(!output.join(".env").exists());
    }

    #[test]
    fn rejects_existing_restore_directory() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("sample");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        let bundle = temp.path().join("sample.blackout");
        create(&project, &bundle).unwrap();
        let output = temp.path().join("existing");
        fs::create_dir(&output).unwrap();
        assert!(restore(&bundle, &output).is_err());
    }

    #[test]
    fn rejects_path_traversal_before_restore() {
        let temp = tempdir().unwrap();
        let bundle = temp.path().join("malicious.blackout");
        let output = File::create(&bundle).unwrap();
        let encoder = zstd::Encoder::new(output, 1).unwrap();
        let mut archive = tar::Builder::new(encoder);
        let contents = b"outside";
        let digest = format!("{:x}", Sha256::digest(contents));
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.as_mut_bytes()[.."../outside.txt".len()].copy_from_slice(b"../outside.txt");
        header.set_cksum();
        archive.append(&header, &contents[..]).unwrap();
        let manifest = BundleManifest {
            format_version: 1,
            project: "fixture".into(),
            ecosystem: "cargo".into(),
            target: Target {
                os: "linux".into(),
                architecture: "x86_64".into(),
            },
            files: vec![FileRecord {
                path: "../outside.txt".into(),
                sha256: digest,
                size: contents.len() as u64,
                mode: 0o644,
            }],
            validation: Validation {
                network_isolated: None,
                build_passed: None,
                tests_passed: None,
            },
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, super::MANIFEST, bytes.as_slice())
            .unwrap();
        archive.finish().unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        let destination = temp.path().join("restore");
        assert!(restore(&bundle, &destination).is_err());
        assert!(!temp.path().join("outside.txt").exists());
    }

    #[test]
    fn detects_payload_corruption() {
        let temp = tempdir().unwrap();
        let project = temp.path().join("sample");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        fs::write(project.join("src.rs"), "fn main() {}\n").unwrap();
        let bundle = temp.path().join("sample.blackout");
        create(&project, &bundle).unwrap();
        let mut bytes = fs::read(&bundle).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&bundle, bytes).unwrap();
        assert!(verify(&bundle).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn restores_safe_executable_permissions() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let project = temp.path().join("sample");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        let script = project.join("run.sh");
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, Permissions::from_mode(0o755)).unwrap();
        let bundle = temp.path().join("sample.blackout");
        create(&project, &bundle).unwrap();
        let restored = temp.path().join("restored");
        restore(&bundle, &restored).unwrap();
        assert_eq!(
            fs::metadata(restored.join("run.sh")).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
