                                                                              
                                                                                 
                                                                          
                                                                              
                                                                                  
                                                                                
                                                                            
                                                                                 
           

use myelin::confine::Root;
use myelin::tools::{Scope, ToolBounds, ToolCall, ToolError, Toolbox};

const SCOPE: &str = "pkg/*/lib.rs";
const SECRET: &str = "OUT-OF-SCOPE-CANARY";

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("pkg/alpha")).unwrap();
    std::fs::write(tmp.path().join("pkg/alpha/lib.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(tmp.path().join("pkg/lib.rs"), format!("{SECRET}\n")).unwrap();
    tmp
}

#[test]
fn setup_the_glob_admits_the_raw_spelling_but_not_the_normalized_path() {
    let s = Scope::new(vec![SCOPE.to_string()]).unwrap();
    assert!(
        s.matcher().is_match("pkg/alpha/lib.rs"),
        "setup: the intended file must match"
    );
    assert!(
        !s.matcher().is_match("pkg/lib.rs"),
        "setup: the normalized out-of-scope path must NOT match"
    );
    assert!(s.matcher().is_match("pkg/./lib.rs"), "raw dot spelling");
    assert!(s.matcher().is_match("pkg//lib.rs"), "raw empty spelling");
}

#[test]
fn read_file_refuses_every_dot_segment_spelling_of_an_out_of_scope_file() {
    let tmp = fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec![SCOPE.to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

    let honest = tb.dispatch(&ToolCall::ReadFile {
        path: "pkg/lib.rs".into(),
        range: None,
    });
    assert!(
        matches!(honest, Err(ToolError::OutOfScope)),
        "control: the direct spelling must be refused"
    );

    let mut leaked = Vec::new();
    for spelling in [
        "pkg/./lib.rs",
        "pkg//lib.rs",
        "./pkg/./lib.rs",
        "pkg/././lib.rs",
    ] {
        let got = tb.dispatch(&ToolCall::ReadFile {
            path: spelling.into(),
            range: None,
        });
        if got.as_deref().is_ok_and(|t| t.contains(SECRET)) {
            leaked.push(spelling);
        }
    }
    assert!(
        leaked.is_empty(),
        "scope bypassed: {leaked:?} returned the content of the out-of-scope pkg/lib.rs"
    );

                                         
    let admitted = tb
        .dispatch(&ToolCall::ReadFile {
            path: "pkg/alpha/lib.rs".into(),
            range: None,
        })
        .expect("in-scope file must still admit");
    assert!(admitted.contains("alpha"));
}

#[test]
fn grep_refuses_every_dot_segment_spelling_of_an_out_of_scope_file() {
    let tmp = fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec![SCOPE.to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

    let honest = tb.dispatch(&ToolCall::Grep {
        pattern: SECRET.into(),
        path: Some("pkg/lib.rs".into()),
    });
    assert!(matches!(honest, Err(ToolError::OutOfScope)), "control");

    let mut leaked = Vec::new();
    for spelling in [
        "pkg/./lib.rs",
        "pkg//lib.rs",
        "./pkg/./lib.rs",
        "pkg/././lib.rs",
    ] {
        let got = tb.dispatch(&ToolCall::Grep {
            pattern: SECRET.into(),
            path: Some(spelling.into()),
        });
        if got.as_deref().is_ok_and(|t| t.contains(SECRET)) {
            leaked.push(spelling);
        }
    }
    assert!(
        leaked.is_empty(),
        "scope bypassed via grep: {leaked:?} returned out-of-scope content"
    );
}

                                                                                 
                                                                                 
                                                                                
                                                              
fn dir_fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("a/pub")).unwrap();
    std::fs::write(tmp.path().join("a/pub/lib.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::write(tmp.path().join("a/lib.rs"), format!("{SECRET}\n")).unwrap();
    tmp
}

#[test]
fn grep_recursive_dot_segment_does_not_return_out_of_scope_content() {
    let tmp = dir_fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["a/*/lib.rs".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

                                                                             
    let clean = tb
        .dispatch(&ToolCall::Grep {
            pattern: SECRET.into(),
            path: Some("a".into()),
        })
        .unwrap();
    assert!(
        !clean.contains(SECRET),
        "control: grep under `a` scanned the out-of-scope a/lib.rs: {clean:?}"
    );

    let mut leaked = Vec::new();
    for spelling in ["a/.", "a/./", "a/././.", "./a/."] {
        let got = tb
            .dispatch(&ToolCall::Grep {
                pattern: SECRET.into(),
                path: Some(spelling.into()),
            })
            .unwrap();
        if got.contains(SECRET) {
            leaked.push(spelling);
        }
    }
    assert!(
        leaked.is_empty(),
        "grep's recursive walk returned out-of-scope content under {leaked:?}"
    );

                                                                             
                                                                   
    let hit = tb
        .dispatch(&ToolCall::Grep {
            pattern: "alpha".into(),
            path: Some("a/.".into()),
        })
        .unwrap();
    assert!(
        hit.contains("a/pub/lib.rs:"),
        "in-scope match not attributed with the normalized path: {hit:?}"
    );
    assert!(
        !hit.contains("a/./"),
        "reply attribution carried the raw dot spelling: {hit:?}"
    );
}

#[test]
fn list_dir_dot_segment_does_not_surface_out_of_scope_name() {
    let tmp = dir_fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["a/*/lib.rs".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

                                                                             
    let base = tb
        .dispatch(&ToolCall::ListDir { path: "a".into() })
        .unwrap();
    assert!(
        base.contains("pub"),
        "control: in-scope dir not listed: {base:?}"
    );
    assert!(
        !base.lines().any(|l| l.contains("lib.rs")),
        "control: list_dir `a` surfaced the out-of-scope file name: {base:?}"
    );

    let mut leaked = Vec::new();
    for spelling in ["a/.", "a/./", "./a/.", "a/././."] {
        let got = tb
            .dispatch(&ToolCall::ListDir {
                path: spelling.into(),
            })
            .unwrap();
        if got.lines().any(|l| l.contains("lib.rs")) {
            leaked.push(spelling);
        }
    }
    assert!(
        leaked.is_empty(),
        "list_dir surfaced the out-of-scope file name under {leaked:?}"
    );

                                                                               
    let inner = tb
        .dispatch(&ToolCall::ListDir {
            path: "a/pub/.".into(),
        })
        .unwrap();
    assert!(
        inner.lines().any(|l| l.contains("lib.rs")),
        "in-scope lib.rs must still list under a/pub: {inner:?}"
    );
}

#[test]
fn a_between_literals_allowlist_shape_is_not_bypassable() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src/inner")).unwrap();
    std::fs::write(tmp.path().join("src/inner/ok.rs"), "fine\n").unwrap();
    std::fs::write(tmp.path().join("src/config.toml"), format!("{SECRET}\n")).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["src/*/**".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    assert!(scope.matcher().is_match("src/inner/ok.rs"), "setup");
    assert!(!scope.matcher().is_match("src/config.toml"), "setup");

    let honest = tb.dispatch(&ToolCall::ReadFile {
        path: "src/config.toml".into(),
        range: None,
    });
    assert!(matches!(honest, Err(ToolError::OutOfScope)), "control");

    let sneaky = tb.dispatch(&ToolCall::ReadFile {
        path: "src/./config.toml".into(),
        range: None,
    });
    assert!(
        !sneaky.as_deref().is_ok_and(|t| t.contains(SECRET)),
        "scope bypassed under the allowlist shape src/*/**: {sneaky:?}"
    );
}
