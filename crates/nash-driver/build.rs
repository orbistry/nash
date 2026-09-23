use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn sources(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "nash") {
            files.push(path);
        }
    }
}

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("base/src");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    sources(&root, &mut files);
    files.sort();
    let mut output = String::from("pub const SOURCES: &[(&str, &str)] = &[\n");
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        output.push_str(&format!(
            "({:?}, include_str!({:?})),\n",
            file.strip_prefix(&root)
                .unwrap()
                .with_extension("")
                .components()
                .map(|part| part.as_os_str().to_str().unwrap())
                .collect::<Vec<_>>()
                .join("."),
            file
        ));
    }
    output.push_str("];\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("base_sources.rs"),
        output,
    )
    .unwrap();
}
