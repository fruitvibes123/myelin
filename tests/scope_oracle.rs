                                                                                
                                                                                   
                                                                                   
                                                                                 
                  
   
                                                                                   
                                                                                  
                                                                               
                                                                                                   
                                                                         

use myelin::confine::Root;
use myelin::tools::{Scope, ToolBounds, ToolCall, ToolError, Toolbox};

fn fixture() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/a.rs"), "let x = 1;\n").unwrap();
    std::fs::create_dir(tmp.path().join("private")).unwrap();
    std::fs::write(tmp.path().join("private/creds.txt"), "SECRET = 1;\n").unwrap();
    std::fs::create_dir(tmp.path().join("empty_out_of_scope")).unwrap();
    tmp
}

#[test]
fn list_dir_out_of_scope_paths_return_one_uniform_refusal() {
    let tmp = fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

                                                                                     
                                                                
    for name in [
        "private",
        "empty_out_of_scope",
        "does_not_exist",
        "private/creds.txt",
    ] {
        assert!(
            matches!(
                tb.dispatch(&ToolCall::ListDir { path: name.into() }),
                Err(ToolError::OutOfScope)
            ),
            "list_dir({name}) did not return the uniform OutOfScope refusal"
        );
    }
                                                                              
    assert!(tb.dispatch(&ToolCall::ListDir { path: ".".into() }).is_ok());
    assert!(
        tb.dispatch(&ToolCall::ListDir { path: "src".into() })
            .is_ok()
    );
}

#[test]
fn single_file_grep_out_of_scope_paths_return_one_uniform_refusal() {
    let tmp = fixture();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["src/**".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

                                                                               
    for p in [
        "private/creds.txt",
        "private/does_not_exist.txt",
        "no_such_dir/x.txt",
        "private",
        "no_such_dir",
    ] {
        assert!(
            matches!(
                tb.dispatch(&ToolCall::Grep {
                    pattern: "SECRET".into(),
                    path: Some(p.into()),
                }),
                Err(ToolError::OutOfScope)
            ),
            "grep(path={p}) did not return the uniform OutOfScope refusal"
        );
    }
                                             
    let hit = tb
        .dispatch(&ToolCall::Grep {
            pattern: "let".into(),
            path: Some("src/a.rs".into()),
        })
        .unwrap();
    assert!(hit.contains("src/a.rs:1"), "{hit}");
}

#[test]
fn list_dir_truncation_note_survives_a_zero_scope() {
                                                                              
                                                                                 
                                                                      
    let tmp = tempfile::tempdir().unwrap();
    for n in 0..5 {
        std::fs::write(tmp.path().join(format!("f{n}.txt")), "x\n").unwrap();
    }
    let root = Root::open(tmp.path()).unwrap();
                                                                                
                                                               
    let scope = Scope::new(vec!["zzz_absent.txt".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds {
            max_list_entries: 2,
            ..ToolBounds::default()
        },
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    let out = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .expect("list_dir dispatch");
    assert!(
        out.contains("(no entries in scope)"),
        "expected the zero-scope line: {out:?}"
    );
    assert!(
        out.contains("[truncated at 2 entries]"),
        "truncation note was hidden behind the scope filter: {out:?}"
    );
}

#[test]
fn wildcard_leading_scope_is_a_partial_oracle() {
                                                                               
                                                                                   
                                                                               
                                                                    
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/a.rs"), "let x = 1;\n").unwrap();
    std::fs::create_dir(tmp.path().join("private")).unwrap();
    std::fs::create_dir(tmp.path().join("private/keys")).unwrap();
    std::fs::write(tmp.path().join("private/creds.txt"), "SECRET = 1;\n").unwrap();
    std::fs::create_dir(tmp.path().join("empty_out_of_scope")).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let scope = Scope::new(vec!["**/*.rs".to_string()]).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: Some(&scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };

                                                                                   
    let listing = tb
        .dispatch(&ToolCall::ListDir {
            path: "private".into(),
        })
        .expect("wildcard scope admits the out-of-scope directory for listing");
    assert!(
        listing.contains("keys"),
        "out-of-scope subdirectory name did not appear: {listing}"
    );

                                                                                 
    assert!(
        tb.dispatch(&ToolCall::ListDir {
            path: "empty_out_of_scope".into(),
        })
        .is_ok(),
        "an existing out-of-scope directory should list under a wildcard scope"
    );
    assert!(
        tb.dispatch(&ToolCall::ListDir {
            path: "does_not_exist".into(),
        })
        .is_err(),
        "an absent path should error, making existence observable"
    );

                                                                                   
                                               
    assert!(
        matches!(
            tb.dispatch(&ToolCall::ReadFile {
                path: "private/creds.txt".into(),
                range: None,
            }),
            Err(ToolError::OutOfScope)
        ),
        "an out-of-scope file's contents must stay refused"
    );
    assert!(
        matches!(
            tb.dispatch(&ToolCall::Grep {
                pattern: "SECRET".into(),
                path: Some("private/creds.txt".into()),
            }),
            Err(ToolError::OutOfScope)
        ),
        "an out-of-scope file's contents must stay refused via grep"
    );
}
