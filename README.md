# nvm-for-windows-rs

A Rust-based, per-user Node.js version manager for Windows. The executable also acts as the `node`, `npm`, `npx`, and `corepack` launcher through file symlinks in a dedicated shim directory.

## Build

Requires Rust and the Windows PowerShell executable. From the repository root:

```powershell
cargo test
cargo build --release
```

## Setup

Set `NVM_HOME` to choose the data directory. By default, data is stored in `%LOCALAPPDATA%\nvm-rs`. Run the release binary once:

```powershell
.\target\release\nvm-for-windows-rs.exe setup
nvm install lts/*
```

`setup` persists `NVM_HOME`, creates `nvm.exe`, `node.exe`, `npm.exe`, `npx.exe`, and `corepack.exe` file symlinks, and prepends the `shims` directory to the current user's PATH. Windows Developer Mode or an elevated terminal may be required to create symlinks. Restart terminals after setup and confirm `where.exe node` resolves to the shims directory; an earlier machine-level Node.js PATH entry must be removed or reordered.

## Version selection

The closest `.nvmrc` found by walking upward from the current directory takes precedence over `settings.json`. `nvm use <version>` changes the persistent default; a project pin still takes precedence. Without a project pin or a configured default, the newest installed version is selected.

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

Supported architectures are `x64`, `arm64`, and `x86`. `npm install -g` writes to a version-specific prefix under `%NVM_HOME%\globals`, isolating global packages between Node.js versions.

## Dispatch

Each shim invokes this executable. It identifies the requested tool from `argv[0]`, resolves the active version, and forwards arguments to that version's `node.exe` or bundled npm CLI. `NVM_BIN`, `NVM_INC`, `NPM_CONFIG_PREFIX`, and `PATH` are set for the child process.