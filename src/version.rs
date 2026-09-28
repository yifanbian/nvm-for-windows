use super::NvmPaths;
use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Deserialize, Serialize)]
pub(super) struct VersionMetadata {
    pub lts: Option<String>,
}

impl NvmPaths {
    pub fn installed_versions(&self) -> Result<Vec<String>> {
        let directory = self.versions_dir();
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut versions = fs::read_dir(&directory)
            .with_context(|| format!("failed to read {}", directory.display()))?
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|version| Version::parse(version.trim_start_matches('v')).is_ok())
            .collect::<Vec<_>>();
        versions.sort_by(|left, right| compare_versions(right, left));
        Ok(versions)
    }

    pub fn select_version(&self, cwd: &Path) -> Result<String> {
        let settings = self.load_settings()?;
        if let Some(requested) = find_nvmrc(cwd)?.or(settings.default) {
            return self.resolve_installed(&requested);
        }
        self.installed_versions()?
            .into_iter()
            .next()
            .context("no Node.js version installed; run `nvm install <version>`")
    }

    pub fn resolve_installed(&self, requested: &str) -> Result<String> {
        let requested = normalize_version(requested)?;
        let versions = self.installed_versions()?;
        if matches!(requested.as_str(), "node" | "latest") {
            return versions
                .into_iter()
                .next()
                .context("no Node.js version installed; run `nvm install node`");
        }
        if requested == "lts" || requested == "lts/*" || requested.starts_with("lts/") {
            let lts_name = requested.strip_prefix("lts/").filter(|name| *name != "*");
            for version in &versions {
                let metadata_path = self.versions_dir().join(version).join(".nvm-release.json");
                let Ok(contents) = fs::read_to_string(metadata_path) else {
                    continue;
                };
                let Ok(metadata) = serde_json::from_str::<VersionMetadata>(&contents) else {
                    continue;
                };
                if metadata.lts.as_deref().is_some_and(|name| {
                    lts_name.is_none_or(|requested_name| name.eq_ignore_ascii_case(requested_name))
                }) {
                    return Ok(version.clone());
                }
            }
            bail!("no installed LTS version matches {requested}; run `nvm install {requested}`")
        }
        if let Some(version) = versions
            .iter()
            .find(|version| version.trim_start_matches('v') == requested)
        {
            return Ok(version.clone());
        }
        let prefix = format!("{requested}.");
        if let Some(version) = versions
            .iter()
            .find(|version| version.trim_start_matches('v').starts_with(&prefix))
        {
            return Ok(version.clone());
        }
        bail!("Node.js {requested} is not installed; run `nvm install {requested}`")
    }

    pub fn use_version(&self, requested: &str) -> Result<String> {
        let version = self.resolve_installed(requested)?;
        let mut settings = self.load_settings()?;
        settings.default = Some(version.clone());
        self.save_settings(&settings)?;
        Ok(version)
    }

    pub fn uninstall(&self, requested: &str) -> Result<String> {
        let version = self.resolve_installed(requested)?;
        let mut settings = self.load_settings()?;
        let remove_default = settings.default.as_deref().is_some_and(|configured| {
            configured == version
                || self
                    .resolve_installed(configured)
                    .is_ok_and(|selected| selected == version)
        });
        fs::remove_dir_all(self.versions_dir().join(&version))?;
        let globals = self.globals_dir(&version);
        if globals.exists() {
            fs::remove_dir_all(globals)?;
        }
        if remove_default {
            settings.default = None;
            self.save_settings(&settings)?;
        }
        Ok(version)
    }
}

pub fn find_nvmrc(start: &Path) -> Result<Option<String>> {
    for directory in start.ancestors() {
        let path = directory.join(".nvmrc");
        if path.is_file() {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let version = contents
                .lines()
                .map(|line| line.split('#').next().unwrap_or_default().trim())
                .find(|line| !line.is_empty())
                .map(str::to_owned);
            if version.is_none() {
                bail!("{} does not contain a version", path.display());
            }
            return Ok(version);
        }
    }
    Ok(None)
}

pub(super) fn normalize_version(requested: &str) -> Result<String> {
    let requested = requested.trim().trim_start_matches('v');
    let valid_alias = matches!(requested, "node" | "latest" | "lts" | "lts/*")
        || requested
            .strip_prefix("lts/")
            .is_some_and(|name| !name.is_empty());
    let numeric = requested
        .split('.')
        .all(|component| !component.is_empty() && component.chars().all(|ch| ch.is_ascii_digit()));
    if requested.is_empty() || (!valid_alias && !numeric) {
        bail!("invalid Node.js version: {requested}");
    }
    Ok(requested.to_owned())
}

pub(super) fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    match (
        Version::parse(left.trim_start_matches('v')),
        Version::parse(right.trim_start_matches('v')),
    ) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn project_nvmrc_overrides_user_default() {
        let temp = tempfile::tempdir().unwrap();
        let paths = NvmPaths {
            root: temp.path().join("nvm"),
        };
        fs::create_dir_all(paths.versions_dir().join("v20.12.0")).unwrap();
        fs::create_dir_all(paths.versions_dir().join("v22.1.0")).unwrap();
        fs::create_dir_all(temp.path().join("project")).unwrap();
        fs::write(paths.settings_file(), r#"{"default":"22"}"#).unwrap();
        fs::write(temp.path().join("project/.nvmrc"), "20\n").unwrap();
        assert_eq!(
            paths.select_version(&temp.path().join("project")).unwrap(),
            "v20.12.0"
        );
    }

    #[test]
    fn finds_parent_nvmrc_and_ignores_comments() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let nested = project.join("src").join("lib");
        fs::create_dir_all(&nested).unwrap();
        fs::write(project.join(".nvmrc"), "# app version\n  20.11 # pinned\n").unwrap();
        assert_eq!(find_nvmrc(&nested).unwrap().as_deref(), Some("20.11"));
    }

    #[test]
    fn installed_versions_are_sorted_semantically() {
        let temp = tempfile::tempdir().unwrap();
        let paths = NvmPaths {
            root: temp.path().to_path_buf(),
        };
        for version in ["v20.9.0", "v20.10.0", "v18.20.4"] {
            fs::create_dir_all(paths.versions_dir().join(version)).unwrap();
        }
        assert_eq!(
            paths.installed_versions().unwrap(),
            ["v20.10.0", "v20.9.0", "v18.20.4"]
        );
    }

    #[test]
    fn lts_alias_selects_latest_installed_matching_line() {
        let temp = tempfile::tempdir().unwrap();
        let paths = NvmPaths {
            root: temp.path().to_path_buf(),
        };
        for (version, lts) in [("v20.18.0", "Iron"), ("v22.11.0", "Jod")] {
            let directory = paths.versions_dir().join(version);
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join(".nvm-release.json"),
                format!(r#"{{"lts":"{lts}"}}"#),
            )
            .unwrap();
        }
        assert_eq!(paths.resolve_installed("lts/*").unwrap(), "v22.11.0");
        assert_eq!(paths.resolve_installed("lts/iron").unwrap(), "v20.18.0");
    }

    #[test]
    fn uninstall_removes_global_packages_and_clears_matching_default() {
        let temp = tempfile::tempdir().unwrap();
        let paths = NvmPaths {
            root: temp.path().to_path_buf(),
        };
        fs::create_dir_all(paths.versions_dir().join("v20.12.0")).unwrap();
        fs::create_dir_all(paths.globals_dir("v20.12.0")).unwrap();
        fs::write(paths.settings_file(), r#"{"default":"20"}"#).unwrap();
        paths.uninstall("20.12.0").unwrap();
        assert!(!paths.versions_dir().join("v20.12.0").exists());
        assert!(!paths.globals_dir("v20.12.0").exists());
        assert!(paths.load_settings().unwrap().default.is_none());
    }
}
