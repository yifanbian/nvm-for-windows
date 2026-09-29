# nvm-for-windows-rs

A Rust-based, per-user Node.js version manager for Windows. The executable also acts as the `node`, `npm`, `npx`, and `corepack` launcher through file symlinks in a dedicated shim directory.

## Build

Requires Rust and the Windows PowerShell executable. From the repository root:

```powershell
cargo test
cargo build --release
```

## Install

Requires PowerShell and an internet connection. Run the installer directly from GitHub:

```powershell
irm https://raw.githubusercontent.com/yifanbian/nvm-for-windows/main/scripts/install.ps1 | iex
nvm install lts/*
```

The installer downloads the executable from the latest GitHub Release and installs it under `%LOCALAPPDATA%\nvm-rs`. It does not require Git or Rust. It also creates command shims and adds the shim directory to the current user's PATH. Set `NVM_HOME` first to choose a different data directory:

```powershell
$env:NVM_HOME = 'D:\Tools\nvm-rs'
irm https://raw.githubusercontent.com/yifanbian/nvm-for-windows/main/scripts/install.ps1 | iex
```

Restart PowerShell after installation. Windows Developer Mode or an elevated terminal may be required to create symlinks. Confirm `where.exe node` resolves to the nvm `shims` directory; an earlier machine-level Node.js PATH entry must be removed or reordered.

Before first installation, a maintainer must push a version tag such as `v0.1.0`. Actions runs the Windows build and tests, then publishes the executable as a GitHub Release asset. Future installer runs download the latest published release.

## Manual Setup

If you already built the release binary, you can configure it manually:

```powershell
.\target\release\nvm-for-windows-rs.exe setup
nvm install lts/*
```

`setup` persists `NVM_HOME`, creates `nvm.exe`, `node.exe`, `npm.exe`, `npx.exe`, and `corepack.exe` file symlinks, and prepends the `shims` directory to the current user's PATH. Windows Developer Mode or an elevated terminal may be required to create symlinks. Restart terminals after setup and confirm `where.exe node` resolves to the shims directory; an earlier machine-level Node.js PATH entry must be removed or reordered.

## Version selection

The closest `.nvmrc` found by walking upward from the current directory takes precedence over `settings.json`. Run `nvm install` inside a project to install the version specified by its `.nvmrc`; an explicit `nvm install <version>` overrides it. `nvm use <version>` changes the persistent default; a project pin still takes precedence. Without a project pin or a configured default, the newest installed version is selected.

```powershell
nvm install 22
nvm use 22
nvm list
nvm current
nvm uninstall 22
```

Settings are stored in `%NVM_HOME%\settings.json`:

```json
{
  "default": "v22.1.0",
  "arch": "x64"
}
```

Supported architectures are `x64`, `arm64`, and `x86`. `npm install -g` writes to a version-specific prefix under `%NVM_HOME%\globals`, isolating global packages between Node.js versions. The corresponding global executable directory is added to the `PATH` of processes launched through an nvm shim, but not to the interactive shell's `PATH`. Therefore, a globally installed CLI may not be directly callable by name from PowerShell. Use a project-local dev dependency with `npx`/`npm exec`, or invoke the CLI through a shim-launched process.

## Dispatch

Each shim invokes this executable. It identifies the requested tool from `argv[0]`, resolves the active version, and forwards arguments to that version's `node.exe` or bundled npm CLI. `NVM_BIN`, `NVM_INC`, `NPM_CONFIG_PREFIX`, and `PATH` are set for the child process.