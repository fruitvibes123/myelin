                                                                       
                                                             

use std::fs;
use std::path::Path;
use std::process::Command;

use myelin::confine::Root;
use myelin::tools::{Scope, ToolBounds, ToolCall, ToolError, Toolbox};

fn toolbox<'a>(root: &'a Root, git: bool, scope: Option<&'a Scope>) -> Toolbox<'a> {
    Toolbox {
        root,
        bounds: ToolBounds::default(),
        scope,
        git_enabled: git,
        search: None,
        excluded_dirs: &[],
    }
}

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

fn git_fixture() -> (tempfile::TempDir, Root) {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("a.txt"), "first\n").unwrap();
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    git(tmp.path(), &["add", "a.txt"]);
    git(tmp.path(), &["commit", "-qm", "myelin-fixture-commit"]);
    let root = Root::open(tmp.path()).unwrap();
    (tmp, root)
}

#[test]
fn git_log_runs_when_enabled_and_is_refused_when_disabled() {
    let (_tmp, root) = git_fixture();

    let out = toolbox(&root, true, None)
        .dispatch(&ToolCall::GitLog { rev_range: None })
        .unwrap();
    assert!(out.contains("myelin-fixture-commit"), "{out}");

    let err = toolbox(&root, false, None)
        .dispatch(&ToolCall::GitLog { rev_range: None })
        .unwrap_err();
    assert!(matches!(err, ToolError::Disabled(_)));
}

#[test]
fn git_diff_accepts_clean_range_and_refuses_injection() {
    let (tmp, root) = git_fixture();
    fs::write(tmp.path().join("a.txt"), "first\nsecond\n").unwrap();
    let tb = toolbox(&root, true, None);

    let out = tb
        .dispatch(&ToolCall::GitDiff {
            rev_range: Some("HEAD".to_string()),
        })
        .unwrap();
    assert!(out.contains("+second"), "{out}");

    for bad in ["HEAD; ls", "--output=/tmp/x", "$(reboot)", "a..b c"] {
        let err = tb
            .dispatch(&ToolCall::GitDiff {
                rev_range: Some(bad.to_string()),
            })
            .unwrap_err();
        assert!(matches!(err, ToolError::BadRevRange), "{bad}");
    }
}

#[test]
fn scope_globs_filter_reads_and_grep_but_cannot_widen_access() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/lib.rs"), "let inside = 1;\n").unwrap();
    fs::write(tmp.path().join("README.md"), "let outside_scope = 2;\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();

    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();
    let tb = toolbox(&root, false, Some(&scope));

                                                         
    assert!(
        tb.dispatch(&ToolCall::ReadFile {
            path: "src/lib.rs".into(),
            range: None
        })
        .is_ok()
    );
    assert!(matches!(
        tb.dispatch(&ToolCall::ReadFile {
            path: "README.md".into(),
            range: None
        }),
        Err(ToolError::OutOfScope)
    ));

                                     
    let out = tb
        .dispatch(&ToolCall::Grep {
            pattern: "let ".into(),
            path: None,
        })
        .unwrap();
    assert!(out.contains("src/lib.rs:1"), "{out}");
    assert!(!out.contains("README.md"), "{out}");

                                                        
    assert!(
        tb.dispatch(&ToolCall::ReadFile {
            path: "../outside.txt".into(),
            range: None
        })
        .is_err()
    );
}

#[test]
fn scope_is_enforced_on_single_file_grep_and_list_dir() {
                                                                             
                                                             
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/lib.rs"), "let inside = 1;\n").unwrap();
    fs::create_dir(tmp.path().join("private")).unwrap();
    fs::write(tmp.path().join("private/creds.txt"), "SECRET = 1;\n").unwrap();
                                                              
    std::os::unix::fs::symlink("/etc/passwd", tmp.path().join("private/pw_link")).unwrap();
    let root = Root::open(tmp.path()).unwrap();

    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();
    let tb = toolbox(&root, false, Some(&scope));

                                                                               
                        
    assert!(matches!(
        tb.dispatch(&ToolCall::Grep {
            pattern: "SECRET".into(),
            path: Some("private/creds.txt".into()),
        }),
        Err(ToolError::OutOfScope)
    ));
                                             
    let hit = tb
        .dispatch(&ToolCall::Grep {
            pattern: "inside".into(),
            path: Some("src/lib.rs".into()),
        })
        .unwrap();
    assert!(hit.contains("src/lib.rs:1"), "{hit}");

                                                                               
                                                                              
                                                                                   
                                                         
    let root_listing = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .unwrap();
    assert!(!root_listing.contains("private"), "{root_listing}");
    assert!(root_listing.contains("src"), "{root_listing}");
                                                                                 
                                                                             
                                                    
    assert!(matches!(
        tb.dispatch(&ToolCall::ListDir {
            path: "private".into(),
        }),
        Err(ToolError::OutOfScope)
    ));
}

#[test]
fn git_tools_are_scope_filtered() {
                                                                                
                                                                                 
                                                                               
    let (tmp, root) = git_fixture();
    fs::create_dir(tmp.path().join("secret")).unwrap();
    fs::write(tmp.path().join("secret/b.txt"), "PRIVATE-CANARY\n").unwrap();
    git(tmp.path(), &["add", "-A"]);
    git(tmp.path(), &["commit", "-qm", "add-secret"]);
    fs::write(tmp.path().join("secret/b.txt"), "PRIVATE-CANARY-CHANGED\n").unwrap();

    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();

                                                                                
    let scoped = toolbox(&root, true, Some(&scope));
    let diff_err = scoped
        .dispatch(&ToolCall::GitDiff { rev_range: None })
        .unwrap_err();
    assert!(matches!(diff_err, ToolError::GitScoped), "{diff_err}");
    let log_err = scoped
        .dispatch(&ToolCall::GitLog { rev_range: None })
        .unwrap_err();
    assert!(matches!(log_err, ToolError::GitScoped), "{log_err}");

                                                                                 
    let open = toolbox(&root, true, None);
    let diff = open
        .dispatch(&ToolCall::GitDiff { rev_range: None })
        .unwrap();
    assert!(diff.contains("PRIVATE-CANARY-CHANGED"), "{diff}");
}

#[test]
fn list_dir_hides_out_of_scope_directory_names() {
                                                                                
                                                                             
                                                                              
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::create_dir(tmp.path().join("customer_acme_secrets")).unwrap();
    fs::write(tmp.path().join("src/a.rs"), "x\n").unwrap();
    fs::write(tmp.path().join("out_of_scope.txt"), "y\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();

    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();
    let tb = toolbox(&root, false, Some(&scope));

    let listing = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .unwrap();
    assert!(!listing.contains("customer_acme_secrets"), "{listing}");
    assert!(!listing.contains("out_of_scope.txt"), "{listing}");
    assert!(listing.contains("src"), "{listing}");
}

#[test]
fn list_dir_shows_wildcard_scope_ancestors() {
                                                                                 
                                                                          
                                                                    
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::create_dir(tmp.path().join("lib")).unwrap();
    fs::write(tmp.path().join("src/a.rs"), "x\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();

    let scope = Scope::new(vec!["**/*.rs".to_string()]).unwrap();
    let tb = toolbox(&root, false, Some(&scope));

    let listing = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .unwrap();
    assert!(listing.contains("src"), "{listing}");
    assert!(listing.contains("lib"), "{listing}");
}
