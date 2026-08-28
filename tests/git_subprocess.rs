                                                                            
                                                                    
                                                                          

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, ToolError, Toolbox};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn no_textconv_blocks_a_repo_local_textconv_driver() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("TEXTCONV_RAN");
    let poison = tmp.path().join("poison.sh");
    script(
        &poison,
        &format!("#!/bin/sh\ntouch {}\ncat \"$1\"\n", marker.display()),
    );

    fs::write(tmp.path().join("a.bin"), b"\x00\x01payload-v1\n").unwrap();
    fs::write(tmp.path().join(".gitattributes"), "a.bin diff=poison\n").unwrap();
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-qm", "seed"]);
                                                                            
    let cfg = tmp.path().join(".git/config");
    let mut text = fs::read_to_string(&cfg).unwrap();
    text.push_str(&format!(
        "[diff \"poison\"]\n\ttextconv = {}\n",
        poison.display()
    ));
    fs::write(&cfg, text).unwrap();
    fs::write(tmp.path().join("a.bin"), b"\x00\x01payload-v2\n").unwrap();

    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: true,
        search: None,
        excluded_dirs: &[],
    };

    let out = tb.dispatch(&ToolCall::GitDiff { rev_range: None }).unwrap();
    assert!(
        !marker.exists(),
        "the repo-local textconv driver executed under --no-textconv"
    );
                                                                                
    assert!(out.contains("a.bin"), "no diff returned: {out}");
}

#[test]
fn end_of_options_stops_an_option_shaped_rev_range() {
                                                                                 
                                                                                 
                                                                               
                                                     
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("TEXTCONV_RAN");
    let poison = tmp.path().join("poison.sh");
    script(
        &poison,
        &format!("#!/bin/sh\ntouch {}\ncat \"$1\"\n", marker.display()),
    );
    fs::write(tmp.path().join("a.bin"), b"\x00\x01payload-v1\n").unwrap();
    fs::write(tmp.path().join(".gitattributes"), "a.bin diff=poison\n").unwrap();
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-qm", "seed"]);
    let cfg = tmp.path().join(".git/config");
    let mut text = fs::read_to_string(&cfg).unwrap();
    text.push_str(&format!(
        "[diff \"poison\"]\n\ttextconv = {}\n",
        poison.display()
    ));
    fs::write(&cfg, text).unwrap();
    fs::write(tmp.path().join("a.bin"), b"\x00\x01payload-v2\n").unwrap();

    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: true,
        search: None,
        excluded_dirs: &[],
    };

    let out = tb.dispatch(&ToolCall::GitDiff {
        rev_range: Some("--textconv".to_string()),
    });
    assert!(
        matches!(out, Err(ToolError::Git(_))),
        "option-shaped rev_range was not refused: {out:?}"
    );
    assert!(
        !marker.exists(),
        "the textconv driver ran — --end-of-options did not neutralize the option-shaped rev_range"
    );
}

#[test]
fn large_git_stderr_does_not_deadlock() {
                                                                                  
                                                                             
                                                                           
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join(".gitattributes"), "* text eol=crlf\n").unwrap();
    for i in 0..800 {
        fs::write(tmp.path().join(format!("f{i:05}.txt")), format!("a{i}\n")).unwrap();
    }
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-qm", "seed"]);
    for i in 0..800 {
        fs::write(tmp.path().join(format!("f{i:05}.txt")), format!("b{i}\n")).unwrap();
    }

    let dir = tmp.path().to_path_buf();
    let (tx, rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let root = Root::open(&dir).unwrap();
        let tb = Toolbox {
            root: &root,
                                                                             
                                                
            bounds: ToolBounds {
                git_deadline_ms: 60_000,
                ..ToolBounds::default()
            },
            scope: None,
            git_enabled: true,
            search: None,
            excluded_dirs: &[],
        };
        let _ = tx.send(tb.dispatch(&ToolCall::GitDiff { rev_range: None }).is_ok());
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(ok) => assert!(ok, "git_diff errored on a large-stderr repo"),
        Err(_) => panic!("git_diff deadlocked draining a large git stderr"),
    }
    let _ = worker.join();
}

#[test]
fn a_blocked_git_is_killed_at_the_deadline() {
                                                                                   
                                                                                
                                                                                    
                                                                             
    let tmp = tempfile::tempdir().unwrap();
    let hook = tmp.path().join("slow_fsmonitor.sh");
    script(&hook, "#!/bin/sh\nsleep 30\n");
    fs::write(tmp.path().join("a.txt"), "one\n").unwrap();
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-qm", "seed"]);
    let cfg = tmp.path().join(".git/config");
    let mut text = fs::read_to_string(&cfg).unwrap();
    text.push_str(&format!("[core]\n\tfsmonitor = {}\n", hook.display()));
    fs::write(&cfg, text).unwrap();
    fs::write(tmp.path().join("a.txt"), "two\n").unwrap();

    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds {
            git_deadline_ms: 500,
            ..ToolBounds::default()
        },
        scope: None,
        git_enabled: true,
        search: None,
        excluded_dirs: &[],
    };

    let start = Instant::now();
    let out = tb.dispatch(&ToolCall::GitDiff { rev_range: None });
    let elapsed = start.elapsed();
    assert!(
        matches!(out, Err(ToolError::GitTimeout(500))),
        "expected GitTimeout, got {out:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the deadline did not bound the call ({elapsed:?})"
    );
}
