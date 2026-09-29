use anyhow::{Context, Result, bail};
use nvm_for_windows_rs::{NvmPaths, ShimCommand, find_nvmrc, run_shim};
use std::ffi::OsString;
use std::path::Path;

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("nvm: {error:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let mut raw_args = std::env::args_os();
    let argv0 = raw_args.next().unwrap_or_default();
    let args = raw_args.collect::<Vec<_>>();
    let paths = NvmPaths::from_env()?;

    if let Some(shim) = ShimCommand::from_argv0(&argv0) {
        return run_shim(&paths, shim, &args);
    }

    let command = args.first().and_then(|arg| arg.to_str()).unwrap_or("help");
    let values = args
        .iter()
        .skip(1)
        .map(os_to_string)
        .collect::<Result<Vec<_>>>()?;
    match command {
        "install" => {
            let requested = install_requested_version(&values, &std::env::current_dir()?)?;
            println!("Installed Node.js {}", paths.install(&requested)?);
        }
        "list" | "ls" => {
            let settings = paths.load_settings()?;
            for version in paths.installed_versions()? {
                let marker = if settings.default.as_deref() == Some(&version) {
                    " *"
                } else {
                    ""
                };
                println!("{version}{marker}");
            }
        }
        "use" => {
            let version = if let Some(requested) = values.first() {
                paths.use_version(requested)?
            } else {
                paths.select_version(&std::env::current_dir()?)?
            };
            println!("Default Node.js set to {version}");
        }
        "current" => println!("{}", paths.select_version(&std::env::current_dir()?)?),
        "which" => println!(
            "{}",
            paths
                .versions_dir()
                .join(paths.select_version(&std::env::current_dir()?)?)
                .join("node.exe")
                .display()
        ),
        "uninstall" => {
            let requested = required(&values, "uninstall <version>")?;
            println!("Uninstalled Node.js {}", paths.uninstall(requested)?);
        }
        "setup" => {
            paths.setup_shims()?;
            println!(
                "Shims are ready. Restart terminals to load NVM_HOME and the updated user PATH. Check that `where.exe node` resolves to the nvm shims directory."
            );
        }
        "--version" | "-V" => println!("{}", env!("CARGO_PKG_VERSION")),
        "help" | "--help" | "-h" => print_help(),
        other => bail!("unknown command `{other}`; run `nvm help`"),
    }
    Ok(0)
}

fn required<'a>(args: &'a [String], usage: &str) -> Result<&'a str> {
    args.first()
        .map(String::as_str)
        .with_context(|| format!("expected `nvm {usage}`"))
}

fn install_requested_version(args: &[String], cwd: &Path) -> Result<String> {
    if let Some(requested) = args.first() {
        return Ok(requested.clone());
    }
    find_nvmrc(cwd)?.with_context(|| {
        "expected `nvm install <version>` or a .nvmrc in the current or a parent directory"
    })
}

fn os_to_string(value: &OsString) -> Result<String> {
    value
        .to_str()
        .map(str::to_owned)
        .context("nvm command arguments must be valid Unicode")
}

fn print_help() {
    println!(
        "nvm-for-windows-rs\n\nCommands:\n  nvm install [version|lts/*]  Install a version, or use the nearest .nvmrc\n  nvm list                     List installed versions\n  nvm use [version]            Set the default or select the project version\n  nvm current                  Print the effective project version\n  nvm which                    Print the selected node.exe path\n  nvm uninstall <version>      Remove a version and its global packages\n  nvm setup                    Create command shims and update user PATH\n\nProject .nvmrc takes precedence over settings.json."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn install_without_argument_uses_nearest_nvmrc() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let nested = project.join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(project.join(".nvmrc"), "26\n").unwrap();

        assert_eq!(install_requested_version(&[], &nested).unwrap(), "26");
    }

    #[test]
    fn explicit_install_version_overrides_nvmrc() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".nvmrc"), "26\n").unwrap();

        assert_eq!(
            install_requested_version(&["24".into()], temp.path()).unwrap(),
            "24"
        );
    }

    #[test]
    fn install_without_argument_or_nvmrc_returns_usage_error() {
        let temp = tempfile::tempdir().unwrap();

        let error = install_requested_version(&[], temp.path()).unwrap_err();
        assert!(error.to_string().contains("nvm install <version>"));
    }
}
