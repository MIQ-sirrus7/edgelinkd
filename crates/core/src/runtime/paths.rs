use std::path::PathBuf;
use std::sync::OnceLock;

static BUILD_DIRECTORY: OnceLock<String> = OnceLock::new();

/// Remember the app's feature-specific Cargo build directory, relative to the executable.
pub fn set_cargo_output(out_dir: &str) {
    // OUT_DIR describes the build host's filesystem, including for cross builds.
    let normalized = out_dir.replace('\\', "/");
    if let Some(name) = normalized.rsplit('/').nth(1) {
        let _ = BUILD_DIRECTORY.set(name.to_owned());
    }
}

pub fn ui_static_dir() -> PathBuf {
    if let Some(build) = BUILD_DIRECTORY.get()
        && let Ok(executable) = std::env::current_exe()
        && let Some(dir) = executable.parent()
    {
        let cargo_assets = dir.join("build").join(build).join("out/ui_static");
        let packaged = dir.join("ui_static");
        return if cargo_assets.is_dir() { cargo_assets } else { packaged };
    }
    let mut static_dir = std::path::PathBuf::from("target").join("ui_static");
    if !static_dir.exists()
        && let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let release_dir = exe_dir.join("ui_static");
        if release_dir.exists() {
            static_dir = release_dir;
        }
    }
    static_dir
}
