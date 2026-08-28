                                                                             
                                                                           
                                                                             
                                                                            
                                                                          
                                                                       
                                                        

use std::os::unix::fs::PermissionsExt;

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, Toolbox};

const LIST_TRUNC: &str = "directory listing was truncated";
const FILE_TRUNC: &str = "read only to the first";

fn toolbox<'a>(root: &'a Root, bounds: ToolBounds) -> Toolbox<'a> {
    Toolbox {
        root,
        bounds,
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    }
}

fn grep(tb: &Toolbox<'_>, pattern: &str, path: Option<&str>) -> String {
    tb.dispatch(&ToolCall::Grep {
        pattern: pattern.into(),
        path: path.map(str::to_string),
    })
    .expect("grep dispatch")
}

#[test]
fn entry_cap_truncation_is_noted_even_with_matches() {
                                                                                  
                                                               
    let tmp = tempfile::tempdir().unwrap();
    for n in ["a", "b", "c"] {
        std::fs::write(tmp.path().join(format!("{n}.txt")), "CANARY\n").unwrap();
    }
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(
        &root,
        ToolBounds {
            max_list_entries: 2,
            ..ToolBounds::default()
        },
    );
    let out = grep(&tb, "CANARY", None);
    assert!(
        out.contains(LIST_TRUNC),
        "a capped listing carried no truncation note: {out:?}"
    );
}

#[test]
fn entry_cap_false_nomatch_is_not_bare() {
                                                                                 
                                                                       
    let tmp = tempfile::tempdir().unwrap();
    for n in ["a", "b", "c"] {
        std::fs::write(tmp.path().join(format!("{n}.txt")), "nothing here\n").unwrap();
    }
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(
        &root,
        ToolBounds {
            max_list_entries: 2,
            ..ToolBounds::default()
        },
    );
    let out = grep(&tb, "CANARY", None);
    assert!(out.contains("(no matches)"), "{out:?}");
    assert_ne!(
        out, "(no matches)\n",
        "bare (no matches) over a truncated listing"
    );
    assert!(
        out.contains(LIST_TRUNC),
        "no truncation note over a truncated listing: {out:?}"
    );
}

#[test]
fn file_byte_cap_is_noted_on_both_entry_points() {
                                                                               
                                                                                 
    let tmp = tempfile::tempdir().unwrap();
    let mut body = "x".repeat(40);
    body.push('\n');
    body.push_str("CANARY\n");
    std::fs::write(tmp.path().join("big.txt"), &body).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(
        &root,
        ToolBounds {
            max_file_bytes: 32,
            ..ToolBounds::default()
        },
    );

    let recursive = grep(&tb, "CANARY", None);
    assert!(recursive.contains("(no matches)"), "{recursive:?}");
    assert!(
        recursive.contains(FILE_TRUNC),
        "recursive grep did not note the byte-cap truncation: {recursive:?}"
    );

    let single = grep(&tb, "CANARY", Some("big.txt"));
    assert!(single.contains("(no matches)"), "{single:?}");
    assert!(
        single.contains(FILE_TRUNC),
        "single-file grep did not note the byte-cap truncation: {single:?}"
    );
}

#[test]
fn unreadable_entry_is_noted() {
                                                                                
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("secret.txt");
    std::fs::write(&p, "CANARY\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
                                                                            
                                                                                   
                                                                                
    assert!(
        std::fs::read(&p).is_err(),
        "secret.txt readable despite mode 000 (running as root?); this gate needs an \
         unprivileged process to exercise the unreadable-entry note"
    );
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(&root, ToolBounds::default());
    let out = grep(&tb, "CANARY", None);
    assert!(out.contains("(no matches)"), "{out:?}");
    assert!(
        out.contains("could not be read"),
        "unreadable entry carried no note: {out:?}"
    );
}

#[test]
fn binary_file_skip_is_noted() {
                                                                             
                            
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("bin.dat"), b"\x00\x01CANARY\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = toolbox(&root, ToolBounds::default());
    let out = grep(&tb, "CANARY", None);
    assert!(out.contains("(no matches)"), "{out:?}");
    assert!(
        out.contains("binary") && out.contains("skipped"),
        "binary skip carried no note: {out:?}"
    );
}
