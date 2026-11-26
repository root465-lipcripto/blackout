use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub struct CacheStatus {
    pub root: PathBuf,
    pub objects: usize,
    pub bytes: u64,
}

pub fn root() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .ok_or_else(|| anyhow::anyhow!("neither XDG_CACHE_HOME nor HOME is set"))?;
    Ok(base.join("blackout").join("objects"))
}

pub fn digest_file(path: &Path) -> Result<(String, u64)> {
    let mut input = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hasher.finalize()), bytes))
}

fn object_path(root: &Path, digest: &str) -> Result<PathBuf> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("digest must be 64 lowercase hexadecimal characters");
    }
    Ok(root.join(&digest[..2]).join(digest))
}

pub fn put(source: &Path) -> Result<(String, u64, PathBuf)> {
    let (digest, size) = digest_file(source)?;
    let root = root()?;
    let destination = object_path(&root, &digest)?;
    let parent = destination.parent().expect("object path has a parent");
    fs::create_dir_all(parent)?;
    if destination.exists() {
        verify(&digest)?;
        return Ok((digest, size, destination));
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = parent.join(format!(".{}.{}.{}.tmp", digest, std::process::id(), nonce));
    let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let mut input = File::open(source)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    let (stored_digest, stored_size) = digest_file(&temporary)?;
    if stored_digest != digest || stored_size != size {
        let _ = fs::remove_file(&temporary);
        bail!("source changed while being copied; cache object was not committed");
    }
    match fs::hard_link(&temporary, &destination) {
        Ok(()) => {
            fs::remove_file(&temporary)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary)?;
            verify(&digest)?;
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
    }
    Ok((digest, size, destination))
}

pub fn verify(digest: &str) -> Result<PathBuf> {
    let path = object_path(&root()?, digest)?;
    let (observed, _) = digest_file(&path).with_context(|| format!("cached object {digest} is missing"))?;
    if observed != digest {
        bail!("cache object corruption: expected {digest}, observed {observed}");
    }
    Ok(path)
}

pub fn status() -> Result<CacheStatus> {
    let root = root()?;
    if !root.exists() {
        return Ok(CacheStatus {
            root,
            objects: 0,
            bytes: 0,
        });
    }
    let mut objects = 0;
    let mut bytes = 0_u64;
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            objects += 1;
            bytes = bytes.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(CacheStatus { root, objects, bytes })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::tempdir;

    use super::digest_file;

    #[test]
    fn sha256_matches_known_vector() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("input");
        fs::write(&file, b"abc").unwrap();
        let (digest, size) = digest_file(&file).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(size, 3);
    }
}
