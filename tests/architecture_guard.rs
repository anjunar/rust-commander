use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn domain_does_not_depend_on_outer_layers() {
    let domain_files = collect_rust_files(Path::new("src/domain"));
    let forbidden = [
        "crate::remote",
        "crate::archive",
        "crate::fs",
        "crate::ui",
        "crate::platform",
    ];

    assert_no_forbidden_imports(&domain_files, &forbidden);
}

#[test]
fn application_does_not_depend_on_ui_layer() {
    let application_files = collect_rust_files(Path::new("src/application"));
    assert_no_forbidden_imports(&application_files, &["crate::ui"]);
}

fn collect_rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    visit_dir(root, &mut files);
    files
}

fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("Could not read {}: {error}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("Could not read a directory entry in {}: {error}", dir.display())
        });
        let path = entry.path();
        if path.is_dir() {
            visit_dir(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn assert_no_forbidden_imports(files: &[PathBuf], forbidden_imports: &[&str]) {
    let mut violations = Vec::new();

    for path in files {
        let content = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("Could not read {}: {error}", path.display()));

        for needle in forbidden_imports {
            if content.contains(needle) {
                violations.push(format!("{} contains forbidden import {needle}", path.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Architecture guard failed:\n{}",
        violations.join("\n")
    );
}
