use super::NvmPaths;
use super::version::{VersionMetadata, compare_versions, normalize_version};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;

impl NvmPaths {
    pub fn install(&self, requested: &str) -> Result<String> {
        let arch = self.architecture()?;
        let client = HttpClient;
        let index: Vec<Release> =
            serde_json::from_slice(&client.get("https://nodejs.org/dist/index.json")?)
                .context("failed to parse Node.js release index")?;
        let release = resolve_release_record(requested, &index, &arch)?;
        let version = release.version.clone();
        let lts = release.lts.as_str().map(str::to_owned);
        let target = self.versions_dir().join(&version);
        if target.is_dir() {
            return Ok(version);
        }

        let filename = format!("node-{version}-win-{arch}.zip");
        let base_url = format!("https://nodejs.org/dist/{version}");
        let archive_url = format!("{base_url}/{filename}");
        let sums_url = format!("{base_url}/SHASUMS256.txt");
        let expected_hash = checksum_for(&client.get_text(&sums_url)?, &filename)?;
        let versions_dir = self.versions_dir();
        fs::create_dir_all(&versions_dir)?;
        let stage = versions_dir.join(format!(".install-{}-{}", version, std::process::id()));
        if stage.exists() {
            fs::remove_dir_all(&stage)?;
        }
        fs::create_dir_all(&stage)?;
        let archive = stage.join(&filename);
        client.download(&archive_url, &archive)?;
        let actual_hash = format!("{:x}", Sha256::digest(fs::read(&archive)?));
        if actual_hash != expected_hash {
            let _ = fs::remove_dir_all(&stage);
            bail!("SHA-256 mismatch for {filename}");
        }

        let extracted = stage.join("extracted");
        fs::create_dir_all(&extracted)?;
        let status = Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
            .arg("Expand-Archive -LiteralPath $env:NVM_ARCHIVE -DestinationPath $env:NVM_DEST -Force")
            .env("NVM_ARCHIVE", &archive)
            .env("NVM_DEST", &extracted)
            .status()
            .context("failed to start PowerShell for ZIP extraction")?;
        if !status.success() {
            let _ = fs::remove_dir_all(&stage);
            bail!("PowerShell failed to extract {filename}");
        }
        let extracted_root = fs::read_dir(&extracted)?
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.is_dir())
            .context("Node.js archive did not contain an installation directory")?;
        fs::rename(&extracted_root, &target)
            .with_context(|| format!("failed to move Node.js into {}", target.display()))?;
        fs::write(
            target.join(".nvm-release.json"),
            serde_json::to_vec(&VersionMetadata { lts })?,
        )?;
        fs::remove_dir_all(&stage)?;

        let mut settings = self.load_settings()?;
        if settings.default.is_none() {
            settings.default = Some(version.clone());
            self.save_settings(&settings)?;
        }
        Ok(version)
    }
}

#[derive(Deserialize)]
struct Release {
    version: String,
    files: Vec<String>,
    lts: serde_json::Value,
}

fn resolve_release_record<'a>(
    requested: &str,
    releases: &'a [Release],
    arch: &str,
) -> Result<&'a Release> {
    let requested = normalize_version(requested)?;
    let platform = format!("win-{arch}");
    let mut candidates = releases
        .iter()
        .filter(|release| release.files.iter().any(|file| file == &platform))
        .filter(|release| release_matches(&requested, release))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| compare_versions(&right.version, &left.version));
    candidates
        .first()
        .copied()
        .with_context(|| format!("no Windows {arch} Node.js release matches {requested}"))
}

fn release_matches(requested: &str, release: &Release) -> bool {
    let version = release.version.trim_start_matches('v');
    match requested {
        "node" | "latest" => true,
        "lts" | "lts/*" => release.lts != serde_json::Value::Bool(false),
        _ if requested.starts_with("lts/") => release
            .lts
            .as_str()
            .is_some_and(|lts| lts.eq_ignore_ascii_case(requested.trim_start_matches("lts/"))),
        _ => version == requested || version.starts_with(&format!("{requested}.")),
    }
}

fn checksum_for(contents: &str, filename: &str) -> Result<String> {
    contents
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            (name == filename).then(|| digest.to_ascii_lowercase())
        })
        .context("archive checksum was not listed by Node.js")
}

struct HttpClient;

impl HttpClient {
    fn get(&self, url: &str) -> Result<Vec<u8>> {
        let output = Command::new("curl.exe")
            .args(["--fail", "--location", "--silent", "--show-error", url])
            .output()
            .context("curl.exe is required to download Node.js")?;
        if !output.status.success() {
            bail!(
                "download failed for {url}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output.stdout)
    }

    fn get_text(&self, url: &str) -> Result<String> {
        String::from_utf8(self.get(url)?).context("server returned non-UTF-8 text")
    }

    fn download(&self, url: &str, destination: &Path) -> Result<()> {
        let status = Command::new("curl.exe")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(destination)
            .arg(url)
            .status()
            .context("curl.exe is required to download Node.js")?;
        if !status.success() {
            bail!("download failed for {url}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_resolution_filters_architecture_and_lts_line() {
        let releases = vec![
            Release {
                version: "v24.3.0".into(),
                files: vec!["win-x64".into()],
                lts: serde_json::json!("Krypton"),
            },
            Release {
                version: "v24.4.0".into(),
                files: vec!["win-arm64".into()],
                lts: serde_json::json!("Krypton"),
            },
            Release {
                version: "v26.0.0".into(),
                files: vec!["win-x64".into()],
                lts: serde_json::Value::Bool(false),
            },
        ];
        assert_eq!(
            resolve_release_record("lts/krypton", &releases, "x64")
                .unwrap()
                .version,
            "v24.3.0"
        );
        assert!(resolve_release_record("26", &releases, "arm64").is_err());
    }

    #[test]
    fn checksum_parser_matches_the_exact_archive_name() {
        let sums = "abcd  node-v24.0.0-win-x64.zip\nef01  node-v24.0.0-win-arm64.zip\n";
        assert_eq!(
            checksum_for(sums, "node-v24.0.0-win-x64.zip").unwrap(),
            "abcd"
        );
        assert!(checksum_for(sums, "node-v24.0.0-win-x86.zip").is_err());
    }
}
