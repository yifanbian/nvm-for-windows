use super::NvmPaths;
use anyhow::{Context, Result, bail};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShimCommand {
    Node,
    Npm,
    Npx,
    Corepack,
}

impl ShimCommand {
    pub fn from_argv0(argv0: &OsStr) -> Option<Self> {
        let name = Path::new(argv0)
            .file_stem()?
            .to_string_lossy()
            .to_ascii_lowercase();
        match name.as_str() {
            "node" => Some(Self::Node),
            "npm" => Some(Self::Npm),
            "npx" => Some(Self::Npx),
            "corepack" => Some(Self::Corepack),
            _ => None,
        }
    }
}

impl NvmPaths {
    pub fn setup_shims(&self) -> Result<()> {
        use std::os::windows::fs::symlink_file;

        let target = std::env::current_exe().context("failed to locate this executable")?;
        let shim_dir = self.root.join("shims");
        fs::create_dir_all(&shim_dir)?;
        for name in ["nvm.exe", "node.exe", "npm.exe", "npx.exe", "corepack.exe"] {
            let link = shim_dir.join(name);
            match fs::symlink_metadata(&link) {
                Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(&link)?,
                Ok(_) => bail!("refusing to replace non-symlink {}", link.display()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            symlink_file(&target, &link).with_context(|| {
                format!(
                    "failed to create {} (enable Windows Developer Mode or run elevated)",
                    link.display()
                )
            })?;
        }
        self.add_shims_to_user_path(&shim_dir)
    }
}

pub fn run_shim(paths: &NvmPaths, shim: ShimCommand, args: &[OsString]) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let version = paths.select_version(&cwd)?;
    let version_dir = paths.versions_dir().join(&version);
    let globals_dir = paths.globals_dir(&version);
    let node = version_dir.join("node.exe");
    let (program, forwarded_args) = match shim {
        ShimCommand::Node => (node.clone(), args.to_vec()),
        ShimCommand::Npm | ShimCommand::Npx | ShimCommand::Corepack => {
            let script = match shim {
                ShimCommand::Npm => version_dir.join("node_modules/npm/bin/npm-cli.js"),
                ShimCommand::Npx => version_dir.join("node_modules/npm/bin/npx-cli.js"),
                ShimCommand::Corepack => version_dir.join("node_modules/corepack/dist/corepack.js"),
                ShimCommand::Node => unreachable!(),
            };
            if !script.is_file() {
                bail!("{} is not available in Node.js {version}", shim_name(&shim));
            }
            let mut forwarded = vec![script.into_os_string()];
            forwarded.extend_from_slice(args);
            (node, forwarded)
        }
    };
    if !program.is_file() {
        bail!("Node.js {version} is incomplete; reinstall it with `nvm install {version}`");
    }

    fs::create_dir_all(&globals_dir)?;
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let path = child_path(&version_dir, &globals_dir, &existing_path)?;
    let status = Command::new(program)
        .args(forwarded_args)
        .env("PATH", path)
        .env("NVM_BIN", &version_dir)
        .env("NVM_INC", version_dir.join("include/node"))
        .env("NPM_CONFIG_PREFIX", &globals_dir)
        .status()?;
    Ok(status.code().unwrap_or(1))
}

fn child_path(version_dir: &Path, globals_dir: &Path, existing_path: &OsStr) -> Result<OsString> {
    let mut entries = vec![version_dir.to_path_buf(), globals_dir.to_path_buf()];
    entries.extend(std::env::split_paths(existing_path));
    Ok(std::env::join_paths(entries)?)
}

fn shim_name(shim: &ShimCommand) -> &'static str {
    match shim {
        ShimCommand::Node => "node",
        ShimCommand::Npm => "npm",
        ShimCommand::Npx => "npx",
        ShimCommand::Corepack => "corepack",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn child_path_prefers_selected_node_before_global_tools() {
        let temp = tempfile::tempdir().unwrap();
        let version = temp.path().join("versions/v26.0.0");
        let globals = temp.path().join("globals/v26.0.0");
        let existing = OsString::from(r"C:\Windows;C:\Tools");
        let entries = std::env::split_paths(&child_path(&version, &globals, &existing).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entries[0], version);
        assert_eq!(entries[1], globals);
        assert_eq!(entries[2], PathBuf::from(r"C:\Windows"));
    }
}
