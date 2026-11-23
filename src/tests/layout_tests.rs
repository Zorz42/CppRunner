use std::path::{Path, PathBuf};

fn rust_files(folder: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn tests_live_only_in_src_tests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for folder in ["tests", "benches", "examples"] {
        assert!(!root.join(folder).exists(), "tests belong in src/tests, not in {folder}/");
    }
    let tests = root.join("src").join("tests");
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    for file in files.iter().filter(|file| !file.starts_with(&tests)) {
        let text = std::fs::read_to_string(file).unwrap().replace("#[cfg(test)]\nmod tests;", "");
        assert!(!text.contains("#[test]") && !text.contains("cfg(test)"), "{} has tests; they belong in src/tests", file.display());
    }
}
