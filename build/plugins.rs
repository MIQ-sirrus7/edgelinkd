use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::{env, fs};

use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    package: Package,
    lib: Option<Library>,
}

#[derive(Deserialize)]
struct Library {
    name: Option<String>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    metadata: Metadata,
}

#[derive(Deserialize)]
struct Metadata {
    edgelink: Editor,
}

#[derive(Deserialize)]
struct Editor {
    format: u32,
    module: String,
    editor: Vec<PathBuf>,
    notice: PathBuf,
}

pub struct Plugin {
    pub library: String,
    module: String,
    files: Vec<PathBuf>,
}

pub fn selected() -> Vec<Plugin> {
    let mut plugins = Vec::new();
    let mut modules = BTreeSet::new();
    for (key, value) in env::vars_os() {
        let key = key.to_string_lossy();
        if !key.starts_with("DEP_EDGELINK_PLUGIN_") || !key.ends_with("_MANIFEST") {
            continue;
        }
        let path = PathBuf::from(value);
        println!("cargo:rerun-if-changed={}", path.display());
        let manifest: Manifest = toml::from_str(&fs::read_to_string(&path).expect("read plugin manifest"))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let library = manifest.lib.and_then(|l| l.name).unwrap_or(manifest.package.name).replace('-', "_");
        let editor = manifest.package.metadata.edgelink;
        assert_eq!(editor.format, 1, "unsupported plugin format in {}", path.display());
        assert_eq!(Path::new(&editor.module).file_name(), Some(OsStr::new(&editor.module)), "invalid plugin module");
        assert!(modules.insert(editor.module.clone()), "duplicate plugin module in {}", path.display());
        let mut files = Vec::new();
        let mut names = BTreeSet::new();
        for relative in editor.editor.into_iter().chain([editor.notice]) {
            assert!(
                relative.components().all(|p| matches!(p, Component::Normal(_))),
                "invalid plugin asset {relative:?}"
            );
            let file = path.parent().unwrap().join(relative);
            assert!(
                file.is_file() && names.insert(file.file_name().unwrap().to_owned()),
                "missing or duplicate asset {file:?}"
            );
            println!("cargo:rerun-if-changed={}", file.display());
            files.push(file);
        }
        plugins.push(Plugin { library, module: editor.module, files });
    }
    plugins.sort_by(|a, b| a.library.cmp(&b.library));
    plugins
}

impl Plugin {
    pub fn stage(&self, root: &Path) {
        let dir = root.join("nodes").join(&self.module);
        fs::create_dir(&dir).unwrap_or_else(|e| panic!("cannot stage plugin directory {dir:?}: {e}"));
        for file in &self.files {
            fs::copy(file, dir.join(file.file_name().unwrap())).expect("copy external plugin asset");
        }
    }
}
