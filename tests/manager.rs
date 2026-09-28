use nvm_for_windows_rs::{NvmPaths, ShimCommand};
use std::ffi::OsStr;
use std::fs;

#[test]
fn shim_name_selects_the_forwarded_command() {
    assert_eq!(
        ShimCommand::from_argv0(OsStr::new(r"C:\nvm\shims\node.exe")),
        Some(ShimCommand::Node)
    );
    assert_eq!(
        ShimCommand::from_argv0(OsStr::new("NPM.EXE")),
        Some(ShimCommand::Npm)
    );
    assert_eq!(
        ShimCommand::from_argv0(OsStr::new("npx.exe")),
        Some(ShimCommand::Npx)
    );
    assert_eq!(
        ShimCommand::from_argv0(OsStr::new("corepack.exe")),
        Some(ShimCommand::Corepack)
    );
    assert_eq!(ShimCommand::from_argv0(OsStr::new("nvm.exe")), None);
}

#[test]
fn project_pin_overrides_the_user_default_and_selects_partial_versions() {
    let temp = tempfile::tempdir().unwrap();
    let paths = NvmPaths {
        root: temp.path().join("nvm"),
    };
    fs::create_dir_all(paths.versions_dir().join("v20.12.0")).unwrap();
    fs::create_dir_all(paths.versions_dir().join("v22.1.0")).unwrap();
    fs::write(paths.settings_file(), r#"{"default":"22.1.0"}"#).unwrap();
    let project = temp.path().join("project").join("nested");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.parent().unwrap().join(".nvmrc"), "20\n").unwrap();

    assert_eq!(paths.select_version(&project).unwrap(), "v20.12.0");
}

#[test]
fn each_version_gets_an_independent_global_package_prefix() {
    let paths = NvmPaths {
        root: "C:/Users/test/AppData/Local/nvm-rs".into(),
    };
    assert_ne!(paths.globals_dir("v20.12.0"), paths.globals_dir("v22.1.0"));
    assert_eq!(
        paths.globals_dir("v20.12.0"),
        paths.root.join("globals/v20.12.0")
    );
}
