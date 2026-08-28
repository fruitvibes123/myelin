                                                                               
                                                                                 
                                                                                   
                                                    

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, Toolbox};

const NOTE: &str = "[skipped";

fn toolbox<'a>(root: &'a Root, excluded: &'a [String]) -> Toolbox<'a> {
    Toolbox {
        root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: excluded,
    }
}

#[test]
fn recursive_grep_notes_skipped_directories() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".github")).unwrap();
    std::fs::write(tmp.path().join(".github/ci.yml"), "run: CANARY\n").unwrap();
    std::fs::create_dir(tmp.path().join("target")).unwrap();
    std::fs::write(tmp.path().join("target/gen.rs"), "// CANARY\n").unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "fn ok() {}\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let excluded = vec!["target".to_string()];
    let tb = toolbox(&root, &excluded);

                                                                                     
    let whole = tb
        .dispatch(&ToolCall::Grep {
            pattern: "CANARY".into(),
            path: None,
        })
        .unwrap();
    assert!(whole.contains("(no matches)"), "{whole}");
    assert!(
        whole.contains(NOTE) && whole.contains("2 dot/excluded directories"),
        "recursive grep did not note the skipped directories: {whole}"
    );

                                                                              
    for p in [".github", "target"] {
        let r = tb
            .dispatch(&ToolCall::Grep {
                pattern: "CANARY".into(),
                path: Some(p.into()),
            })
            .unwrap();
        assert!(
            r.contains("CANARY"),
            "direct grep({p}) missed the match: {r}"
        );
        assert!(
            !r.contains(NOTE),
            "direct grep({p}) should carry no skip note: {r}"
        );
    }
}

#[test]
fn recursive_grep_with_nothing_to_skip_carries_no_note() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "// CANARY\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(&root, &[]);
    let whole = tb
        .dispatch(&ToolCall::Grep {
            pattern: "CANARY".into(),
            path: None,
        })
        .unwrap();
    assert!(whole.contains("CANARY"), "{whole}");
    assert!(
        !whole.contains(NOTE),
        "no dirs were skipped; there must be no note: {whole}"
    );
}
