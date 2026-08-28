                                                                             
                                                                                 
                                                                            
                                                                          
                                                            

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use myelin::confine::Root;
use myelin::tools::{Scope, ToolBounds, ToolCall, Toolbox};

fn grep_under_scope(dir: &Path, globs: &[&str]) -> String {
    let root = Root::open(dir).unwrap();
    let scope = Scope::new(globs.iter().map(|s| s.to_string()).collect()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &["target".to_string()],
    };
    tb.dispatch(&ToolCall::Grep {
        pattern: "needle".into(),
        path: None,
    })
    .unwrap()
}

fn in_scope_only(base: &Path) {
    std::fs::create_dir(base.join("src")).unwrap();
    std::fs::write(base.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
}

#[test]
fn out_of_scope_dot_and_excluded_directories_do_not_change_the_reply() {
    let a = tempfile::tempdir().unwrap();
    in_scope_only(a.path());
    let ra = grep_under_scope(a.path(), &["src/**"]);

    let b = tempfile::tempdir().unwrap();
    in_scope_only(b.path());
    std::fs::create_dir(b.path().join(".secrets")).unwrap();
    std::fs::create_dir(b.path().join("target")).unwrap();
    let rb = grep_under_scope(b.path(), &["src/**"]);

    assert_eq!(
        ra, rb,
        "scoped grep distinguishes two trees identical inside the scope: \
         out-of-scope dot/excluded dirs were counted"
    );
}

#[test]
fn an_out_of_scope_unlistable_subdirectory_does_not_change_the_reply() {
    let a = tempfile::tempdir().unwrap();
    in_scope_only(a.path());
    std::fs::create_dir_all(a.path().join("vault/inner")).unwrap();
    let ra = grep_under_scope(a.path(), &["src/**"]);

    let b = tempfile::tempdir().unwrap();
    in_scope_only(b.path());
    std::fs::create_dir_all(b.path().join("vault/inner")).unwrap();
    std::fs::set_permissions(
        b.path().join("vault/inner"),
        std::fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let rb = grep_under_scope(b.path(), &["src/**"]);
    let _ = std::fs::set_permissions(
        b.path().join("vault/inner"),
        std::fs::Permissions::from_mode(0o755),
    );

    assert_eq!(
        ra, rb,
        "scoped grep reveals that an out-of-scope subdirectory is unlistable"
    );
}

#[test]
fn an_out_of_scope_large_directory_does_not_change_the_reply() {
    let bounds_max = ToolBounds::default().max_list_entries;

    let a = tempfile::tempdir().unwrap();
    in_scope_only(a.path());
    std::fs::create_dir(a.path().join("data")).unwrap();
    for i in 0..10 {
        std::fs::write(a.path().join(format!("data/f{i}.bin")), "x").unwrap();
    }
    let ra = grep_under_scope(a.path(), &["src/**"]);

    let b = tempfile::tempdir().unwrap();
    in_scope_only(b.path());
    std::fs::create_dir(b.path().join("data")).unwrap();
    for i in 0..=bounds_max {
        std::fs::write(b.path().join(format!("data/f{i}.bin")), "x").unwrap();
    }
    let rb = grep_under_scope(b.path(), &["src/**"]);

    assert_eq!(
        ra, rb,
        "scoped grep reveals the size class of an out-of-scope directory"
    );
}
