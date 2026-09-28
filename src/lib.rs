mod config;
mod install;
mod shim;
mod version;

pub use config::{NvmPaths, Settings};
pub use shim::{ShimCommand, run_shim};
pub use version::find_nvmrc;
