use anyhow::{Context, Result, bail};
use nvm_for_windows_rs::{NvmPaths, ShimCommand, run_shim};
use std::ffi::OsString;

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
            let requested = required(&values, "install <version>")?;
            println!("Installed Node.js {}", paths.install(requested)?);
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

fn os_to_string(value: &OsString) -> Result<String> {
    value
        .to_str()
        .map(str::to_owned)
        .context("nvm command arguments must be valid Unicode")
}

fn print_help() {
    println!(
        "nvm-for-windows-rs\n\nCommands:\n  nvm install <version|lts/*>  Download and install Node.js\n  nvm list                     List installed versions\n  nvm use [version]            Set the default or select the project version\n  nvm current                  Print the effective project version\n  nvm which                    Print the selected node.exe path\n  nvm uninstall <version>      Remove a version and its global packages\n  nvm setup                    Create command shims and update user PATH\n\nProject .nvmrc takes precedence over settings.json."
    );
}
