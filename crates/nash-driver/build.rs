use std::{env, fs, path::PathBuf};

fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("base/src");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "nash"))
        .collect();
    files.sort();
    let mut output = String::from("pub const SOURCES: &[(&str, &str)] = &[\n");
    for file in files {
        println!("cargo:rerun-if-changed={}", file.display());
        output.push_str(&format!(
            "({:?}, include_str!({:?})),\n",
            file.file_stem().unwrap().to_str().unwrap(),
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
