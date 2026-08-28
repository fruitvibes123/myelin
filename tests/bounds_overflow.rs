                                                                             
                                                                            
                                                                              
                                                                                  

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, Toolbox};

#[test]
fn max_file_bytes_u64_max_reads_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), "line one\nline two\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds {
            max_file_bytes: u64::MAX,
            ..ToolBounds::default()
        },
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    let out = tb
        .dispatch(&ToolCall::ReadFile {
            path: "a.txt".into(),
            range: None,
        })
        .expect("read_file must not overflow at max_file_bytes = u64::MAX");
    assert!(out.contains("line one"), "file content missing: {out}");
    assert!(out.contains("line two"), "file content missing: {out}");
}

#[test]
fn config_accepts_max_file_bytes_u64_max() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("c.json");
    std::fs::write(
        &path,
        r#"{ "model_endpoint": "http://127.0.0.1:1/v1",
             "bounds": { "max_file_bytes": 18446744073709551615 } }"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                                                                                     
    let cfg = myelin::config::load(&path).expect("config with u64::MAX bound must load");
    assert_eq!(cfg.bounds.max_file_bytes, u64::MAX);
}

#[test]
fn max_git_bytes_u64_max_returns_without_panic() {
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    let g = |args: &[&str]| {
        let st = Command::new("git")
            .current_dir(tmp.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .args(args)
            .status()
            .unwrap();
        assert!(st.success());
    };
    std::fs::write(tmp.path().join("a.txt"), "one\n").unwrap();
    g(&["init", "-q", "-b", "main"]);
    g(&["add", "-A"]);
    g(&["commit", "-qm", "seed"]);
    std::fs::write(tmp.path().join("a.txt"), "two\n").unwrap();

    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds {
            max_git_bytes: u64::MAX,
            ..ToolBounds::default()
        },
        scope: None,
        git_enabled: true,
        search: None,
        excluded_dirs: &[],
    };
                                                                                       
                                                                 
    let out = tb
        .dispatch(&ToolCall::GitDiff { rev_range: None })
        .expect("git_diff must not overflow at max_git_bytes = u64::MAX");
    assert!(out.contains("two"), "diff content missing: {out}");
}
