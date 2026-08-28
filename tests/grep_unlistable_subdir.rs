                                                                               
                                                                                 
                                                                                    

use std::os::unix::fs::PermissionsExt;

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, Toolbox};

fn toolbox(root: &Root) -> Toolbox<'_> {
    Toolbox {
        root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    }
}

#[test]
fn unlistable_subdir_is_noted_and_matches_survive() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), "CANARY here\n").unwrap();
    let bad = tmp.path().join("zz_locked");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("inner.txt"), "CANARY inner\n").unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o000)).unwrap();

                                                                                 
                                                                                   
                                  
    let listable = std::fs::read_dir(&bad).is_ok();
    if listable {
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        panic!(
            "zz_locked listable despite mode 000 (running as root?); this gate needs an \
             unprivileged process to exercise the unlistable-subdir path"
        );
    }

    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(&root);
    let out = tb.dispatch(&ToolCall::Grep {
        pattern: "CANARY".into(),
        path: None,
    });

                                                                             
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();

    let text = out.expect("grep must not fail on an unlistable sub-directory");
    assert!(
        text.contains("a.txt:1: CANARY here"),
        "the match found before the unlistable dir was dropped: {text:?}"
    );
    assert!(
        text.contains("could not be listed"),
        "the unlistable sub-directory carried no note: {text:?}"
    );
}
