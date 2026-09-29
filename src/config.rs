use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ, RegCloseKey, RegCreateKeyW, RegQueryValueExW,
    RegSetValueExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};

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
        let subkey = wide_null("Environment");
        let mut key = null_mut();
        let status = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, subkey.as_ptr(), &mut key) };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32))
                .context("failed to open the current user's Environment registry key");
        }

        let result = (|| {
            write_registry_string(key, "NVM_HOME", &self.root.to_string_lossy(), REG_SZ)?;

            let existing = read_registry_string(key, "Path")?;
            let (existing_path, value_type) = existing
                .as_ref()
                .map(|(value, kind)| (Some(value.as_str()), *kind))
                .unwrap_or((None, REG_EXPAND_SZ));
            let updated = prepend_path_entry(existing_path, &shim_dir.to_string_lossy());
            write_registry_string(key, "Path", &updated, value_type)?;
            broadcast_environment_change()
        })();

        unsafe { RegCloseKey(key) };
        result
    }
}

fn read_registry_string(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
) -> Result<Option<(String, u32)>> {
    let name = wide_null(name);
    let mut value_type = 0;
    let mut size = 0;
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            null(),
            &mut value_type,
            null_mut(),
            &mut size,
        )
    };
    if status == 2 {
        return Ok(None);
    }
    if status != 0 && status != 234 {
        return Err(io::Error::from_raw_os_error(status as i32))
            .context("failed to read a value from the user Environment registry key");
    }
    if value_type != REG_SZ && value_type != REG_EXPAND_SZ {
        bail!("user Environment registry value is not a string");
    }

    let mut data = vec![0u16; (size as usize).div_ceil(std::mem::size_of::<u16>()) + 1];
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            null(),
            &mut value_type,
            data.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32))
            .context("failed to read a value from the user Environment registry key");
    }
    let length = data
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(data.len());
    Ok(Some((
        String::from_utf16_lossy(&data[..length]),
        value_type,
    )))
}

fn write_registry_string(
    key: windows_sys::Win32::System::Registry::HKEY,
    name: &str,
    value: &str,
    value_type: u32,
) -> Result<()> {
    let name = wide_null(name);
    let data = wide_null(value);
    let byte_count = data
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|size| u32::try_from(size).ok())
        .context("registry value is too large")?;
    let status = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            value_type,
            data.as_ptr().cast(),
            byte_count,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32))
            .with_context(|| format!("failed to write user Environment value {name:?}"));
    }
    Ok(())
}

fn broadcast_environment_change() -> Result<()> {
    let environment = wide_null("Environment");
    let mut result = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            environment.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        )
    };
    if sent == 0 {
        return Err(io::Error::last_os_error())
            .context("failed to broadcast the environment change");
    }
    Ok(())
}

fn prepend_path_entry(existing: Option<&str>, entry: &str) -> String {
    let existing = existing.unwrap_or_default().trim_matches(';');
    let already_present = existing.split(';').any(|candidate| {
        candidate
            .trim_end_matches(['\\', '/'])
            .eq_ignore_ascii_case(entry.trim_end_matches(['\\', '/']))
    });
    if already_present {
        existing.to_owned()
    } else if existing.is_empty() {
        entry.to_owned()
    } else {
        format!("{entry};{existing}")
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::prepend_path_entry;

    #[test]
    fn prepends_shim_path_without_duplicate() {
        assert_eq!(
            prepend_path_entry(Some(r"C:\Windows;C:\nvm\shims"), r"c:\nvm\SHIMS\"),
            r"C:\Windows;C:\nvm\shims"
        );
    }

    #[test]
    fn prepends_shim_path_and_keeps_existing_entries() {
        assert_eq!(
            prepend_path_entry(Some(r"C:\Windows"), r"C:\nvm\shims"),
            r"C:\nvm\shims;C:\Windows"
        );
    }
}
