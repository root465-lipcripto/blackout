use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct Ecosystem {
    pub name: String,
    pub supported: bool,
    pub markers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectReport {
    pub name: String,
    pub path: PathBuf,
    pub status: &'static str,
    pub ambiguous: bool,
    pub ecosystems: Vec<Ecosystem>,
    pub workspace: bool,
}

pub fn scan(path: &Path) -> Result<ProjectReport> {
    let root = path
        .canonicalize()
        .with_context(|| format!("{} does not exist", path.display()))?;
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }

    let markers: [(&str, &[&str], bool); 4] = [
        ("cargo", &["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"], true),
        (
            "python",
            &[
                "pyproject.toml",
                "requirements.txt",
                "Pipfile",
                "poetry.lock",
                "uv.lock",
            ],
            false,
        ),
        (
            "node",
            &["package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"],
            false,
        ),
        ("go", &["go.mod", "go.sum", "go.work", "vendor/modules.txt"], false),
    ];
    let mut found = Vec::new();
    for (ecosystem, names, supported) in markers {
        let mut hits = Vec::new();
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .max_depth(4)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || (!entry.file_type().is_symlink() && !excluded_dir(entry.file_name().to_string_lossy().as_ref()))
            })
        {
            let entry = entry?;
            if entry.file_type().is_file() && names.iter().any(|name| entry.file_name() == *name) {
                hits.push(entry.path().strip_prefix(&root)?.display().to_string());
            }
        }
        if !hits.is_empty() {
            found.push(Ecosystem {
                name: ecosystem.into(),
                supported,
                markers: hits,
            });
        }
    }

    let cargo_toml = root.join("Cargo.toml");
    let workspace = std::fs::read_to_string(&cargo_toml)
        .map(|content| content.lines().any(|line| line.trim() == "[workspace]"))
        .unwrap_or(false);
    let supported = found.iter().any(|item| item.name == "cargo");
    let status = if found.is_empty() {
        "UNSUPPORTED"
    } else if supported {
        "DETECTED_CARGO"
    } else {
        "DETECTED_ONLY"
    };
    Ok(ProjectReport {
        name: root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_owned(),
        path: root,
        status,
        ambiguous: found.len() > 1,
        ecosystems: found,
        workspace,
    })
}

pub(crate) fn excluded_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".venv" | "venv" | "dist" | "build"
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::scan;

    #[test]
    fn detects_cargo_and_other_ecosystems_without_following_noise_dirs() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname='fixture'\n").unwrap();
        fs::write(dir.path().join("package.json"), "{}\n").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target/package-lock.json"), "{}\n").unwrap();
        let report = scan(dir.path()).unwrap();
        assert!(report.ambiguous);
        assert_eq!(report.status, "DETECTED_CARGO");
        assert_eq!(report.ecosystems.len(), 2);
        assert!(
            !report
                .ecosystems
                .iter()
                .any(|ecosystem| ecosystem.markers.iter().any(|marker| marker.starts_with("target/")))
        );
    }
}
