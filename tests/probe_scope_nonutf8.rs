                                                                                
                                                                               
                                                                            
                                                                         

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

use myelin::confine::Root;
use myelin::tools::{Scope, ScopeProbe, ToolBounds, probe_scope};

fn scope(globs: &[&str]) -> Scope {
    Scope::new(globs.iter().map(|s| s.to_string()).collect()).unwrap()
}

#[test]
fn probe_scope_is_not_empty_under_a_non_utf8_directory_name() {
    let tmp = tempfile::tempdir().unwrap();
                                                                                   
    let bad = tmp.path().join(OsStr::from_bytes(b"\xff"));
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("hit.rs"), "fn f() {}\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let b = ToolBounds::default();

    let p = probe_scope(&root, &b, &scope(&["**/*.rs"]));

                                                                               
    let tmp2 = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp2.path().join("ok")).unwrap();
    std::fs::write(tmp2.path().join("ok/hit.rs"), "fn f() {}\n").unwrap();
    let root2 = Root::open(tmp2.path()).unwrap();
    assert_eq!(
        probe_scope(&root2, &b, &scope(&["**/*.rs"])),
        ScopeProbe::Matched,
        "control must match"
    );

    assert_ne!(
        p,
        ScopeProbe::Empty,
        "probe_scope reported Empty though <0xff>/hit.rs matches **/*.rs: \
         the lossy-name bail must degrade to Unknown, not assert a certain negative"
    );
}
