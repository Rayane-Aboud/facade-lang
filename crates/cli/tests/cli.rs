use std::process::{Command, Output};

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn facadec(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_facadec"))
        .args(args)
        .current_dir(ROOT)
        .output()
        .expect("failed to start facadec")
}

#[test]
fn check_accepts_example() {
    let out = facadec(&["check", "examples/ok/fig5_panel.fcd"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "ok: examples/ok/fig5_panel.fcd\n"
    );
}

#[test]
fn missing_file_is_an_io_error() {
    let out = facadec(&["check", "does-not-exist.fcd"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot read `does-not-exist.fcd`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn no_arguments_prints_usage() {
    let out = facadec(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("usage: facadec check"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_rejects_program_with_syntax_error() {
    let out = facadec(&["check", "examples/err/param_missing_type.fcd"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    for expected in [
        "[E0002] Error: expected `:`, found `=`",
        "examples/err/param_missing_type.fcd:4:14",
        "param offset = 0.0 m",
    ] {
        assert!(
            stderr.contains(expected),
            "missing {expected:?} in: {stderr}"
        );
    }
}
