                                                                                
                                                                                  
                                                                     

use myelin::confine::Root;
use myelin::tools::{Scope, ToolBounds, ToolCall, Toolbox};

fn toolbox<'a>(root: &'a Root, scope: &'a Scope) -> Toolbox<'a> {
    Toolbox {
        root,
        bounds: ToolBounds::default(),
        scope: Some(scope),
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    }
}

                                                                                 
                                                              
#[test]
fn a_scope_matching_the_raw_name_admits_it_everywhere() {
    const WEIRD: &str = "x\nFORGED.rs";
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(WEIRD), "// CANARY\n").unwrap();
    std::fs::write(tmp.path().join("plain.rs"), "// nothing\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();

                                                                                    
    let scope = Scope::new(vec!["x?FORGED.rs".to_string()]).unwrap();
    assert!(
        scope.matcher().is_match(WEIRD),
        "setup: glob matches raw name"
    );
    let tb = toolbox(&root, &scope);

                                     
    let read = tb.dispatch(&ToolCall::ReadFile {
        path: WEIRD.to_string(),
        range: None,
    });
    assert!(read.is_ok(), "read_file refused an in-scope name: {read:?}");

                                                
    let listing = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .unwrap();
    assert!(
        listing.contains("x<0x0A>FORGED.rs"),
        "list_dir hid an in-scope name: {listing:?}"
    );

                                                                         
    let grep = tb
        .dispatch(&ToolCall::Grep {
            pattern: "CANARY".into(),
            path: None,
        })
        .unwrap();
    assert!(
        grep.contains("x<0x0A>FORGED.rs:1: // CANARY"),
        "grep did not admit an in-scope name: {grep:?}"
    );
}

                                                                             
                                                                            
                                                                  
#[test]
fn a_scope_matching_only_the_rendering_refuses_it_everywhere() {
    const RAW: &str = "a\tb.rs";                                  
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(RAW), "// SECRET\n").unwrap();
    std::fs::write(tmp.path().join("plain.rs"), "// nothing\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();

    let scope = Scope::new(vec!["*0x*".to_string()]).unwrap();
    assert!(
        !scope.matcher().is_match(RAW),
        "setup: glob must NOT match the raw name"
    );
    assert!(
        scope.matcher().is_match("a<0x09>b.rs"),
        "setup: glob must match the escaped rendering"
    );
    let tb = toolbox(&root, &scope);

                                      
    let read = tb.dispatch(&ToolCall::ReadFile {
        path: RAW.to_string(),
        range: None,
    });
    assert!(
        read.is_err(),
        "read_file admitted an out-of-scope name: {read:?}"
    );

                                                          
    let listing = tb
        .dispatch(&ToolCall::ListDir { path: ".".into() })
        .unwrap();
    assert!(
        !listing.contains("a<0x09>b.rs"),
        "list_dir surfaced a name the scope does not admit: {listing:?}"
    );

                             
    let grep = tb
        .dispatch(&ToolCall::Grep {
            pattern: "SECRET".into(),
            path: None,
        })
        .unwrap();
    assert!(
        grep.contains("(no matches)"),
        "grep scanned an out-of-scope name: {grep:?}"
    );
}
