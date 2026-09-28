use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NvmPaths {
    pub root: PathBuf,
}

impl NvmPaths {
    pub fn from_env() -> Result<Self> {
        if let Some(root) = std::env::var_os("NVM_HOME") {
            let root = PathBuf::from(root);
            if !root.is_absolute() {
                bail!("NVM_HOME must be an absolute path");
            }
            return Ok(Self { root });
        }
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA is not set; set NVM_HOME to choose the nvm data directory")?;
        Ok(Self {
            root: base.join("nvm-rs"),
        })
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn globals_dir(&self, version: &str) -> PathBuf {
        self.root.join("globals").join(version)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let path = self.settings_file();
        if !path.exists() {
            return Ok(Settings::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("invalid settings file {}", path.display()))
    }

    pub(crate) fn save_settings(&self, settings: &Settings) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let contents = serde_json::to_string_pretty(settings)?;
        fs::write(self.settings_file(), format!("{contents}\n"))
            .with_context(|| format!("failed to write {}", self.settings_file().display()))
    }

    pub(crate) fn architecture(&self) -> Result<String> {
        let settings = self.load_settings()?;
        let arch = settings.arch.unwrap_or_else(|| {
            if cfg!(target_arch = "aarch64") {
                "arm64".into()
            } else if cfg!(target_arch = "x86") {
                "x86".into()
            } else {
                "x64".into()
            }
        });
        if !["x64", "arm64", "x86"].contains(&arch.as_str()) {
            bail!("unsupported architecture in settings.json: {arch}");
        }
        Ok(arch)
    }

    pub(crate) fn add_shims_to_user_path(&self, shim_dir: &Path) -> Result<()> {
        let script = "[Environment]::SetEnvironmentVariable('NVM_HOME',$env:NVM_ROOT,'User'); $p=[string][Environment]::GetEnvironmentVariable('Path','User'); $s=$env:NVM_SHIMS; if (($p -split ';') -notcontains $s) { [Environment]::SetEnvironmentVariable('Path', (($s + ';' + $p.Trim(';')).Trim(';')), 'User') }";
        let status = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env("NVM_SHIMS", shim_dir)
            .env("NVM_ROOT", &self.root)
            .status()
            .context("failed to update the user PATH")?;
        if !status.success() {
            bail!("PowerShell could not update the user PATH");
        }
        Ok(())
    }
}
