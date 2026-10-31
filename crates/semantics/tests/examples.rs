use std::fs;
use std::path::{Path, PathBuf};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples");

#[test]
fn ok_examples_are_accepted() {
    let files = fcd_files("ok");
    assert!(!files.is_empty(), "no examples in examples/ok");
    for path in files {
        let source = fs::read_to_string(&path).expect("readable example");
        let diagnostics = semantics::check(&source);
        assert_eq!(diagnostics, vec![], "{} should be accepted", path.display());
    }
}

#[test]
fn err_examples_report_their_expected_code() {
    let files = fcd_files("err");
    assert!(!files.is_empty(), "no examples in examples/err");
    for path in files {
        let source = fs::read_to_string(&path).expect("readable example");
        let expected = expected_code(&source).unwrap_or_else(|| {
            panic!("{}: first line must be `-- expect: <code>`", path.display())
        });
        let codes: Vec<&str> = semantics::check(&source).iter().map(|d| d.code).collect();
        assert_eq!(codes, vec![expected], "{}", path.display());
    }
}

fn fcd_files(dir: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(Path::new(EXAMPLES).join(dir))
        .expect("examples folder exists")
        .map(|entry| entry.expect("readable folder entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "fcd"))
        .collect();
    files.sort();
    files
}

fn expected_code(source: &str) -> Option<&str> {
    source
        .lines()
        .next()?
        .strip_prefix("-- expect:")
        .map(str::trim)
}
