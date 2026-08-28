                                                                          
                                          

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use myelin::confine::{ConfineError, ConfinedPath, EntryKind, Root};

fn setup() -> (tempfile::TempDir, Root) {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        b"pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();
    let root = Root::open(tmp.path()).unwrap();
    (tmp, root)
}

#[test]
fn rejects_parent_traversal() {
    let (_tmp, root) = setup();
    assert!(matches!(
        ConfinedPath::within(&root, Path::new("../../etc/passwd")),
        Err(ConfineError::ParentTraversal)
    ));
}

#[test]
fn rejects_absolute() {
    let (_tmp, root) = setup();
    assert!(matches!(
        ConfinedPath::within(&root, Path::new("/etc/passwd")),
        Err(ConfineError::Absolute)
    ));
}

#[test]
fn rejects_mixed_traversal() {
    let (_tmp, root) = setup();
    assert!(matches!(
        ConfinedPath::within(&root, Path::new("a/../../etc")),
        Err(ConfineError::ParentTraversal)
    ));
}

#[test]
fn rejects_nul_bearing_name() {
    let (_tmp, root) = setup();
    let raw = Path::new(OsStr::from_bytes(b"src/li\0b.rs"));
    assert!(matches!(
        ConfinedPath::within(&root, raw),
        Err(ConfineError::Nul)
    ));
}

#[test]
fn rejects_empty() {
    let (_tmp, root) = setup();
    assert!(matches!(
        ConfinedPath::within(&root, Path::new("")),
        Err(ConfineError::Empty)
    ));
}

#[test]
fn nonexistent_path_is_not_found() {
    let (_tmp, root) = setup();
    assert!(matches!(
        ConfinedPath::within(&root, Path::new("src/nope.rs")),
        Err(ConfineError::NotFound)
    ));
}

#[test]
fn accepts_clean_path_and_reads_bytes() {
    let (_tmp, root) = setup();
    let p = ConfinedPath::within(&root, Path::new("src/lib.rs")).unwrap();
    let bytes = root.read_confined(&p, 1 << 16).unwrap();
    assert_eq!(bytes, b"pub fn answer() -> u32 { 42 }\n");
}

#[test]
fn read_is_capped_at_max_bytes() {
    let (_tmp, root) = setup();
    let p = ConfinedPath::within(&root, Path::new("src/lib.rs")).unwrap();
    let bytes = root.read_confined(&p, 10).unwrap();
    assert_eq!(bytes, b"pub fn ans");
}

#[test]
fn read_of_directory_is_refused() {
    let (_tmp, root) = setup();
    let p = ConfinedPath::within(&root, Path::new("src")).unwrap();
    assert!(matches!(
        root.read_confined(&p, 1 << 16),
        Err(ConfineError::NotRegularFile)
    ));
}

#[test]
fn list_dir_returns_entries_and_caps() {
    let (tmp, root) = setup();
    fs::write(tmp.path().join("src/extra.rs"), b"").unwrap();
    let p = ConfinedPath::within(&root, Path::new("src")).unwrap();

    let mut entries = root.list_confined(&p, 64).unwrap();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["extra.rs", "lib.rs"]);
    assert!(entries.iter().all(|e| e.kind == EntryKind::File));

    assert_eq!(root.list_confined(&p, 1).unwrap().len(), 1);
    assert!(root.list_confined(&p, 0).unwrap().is_empty());
}

#[test]
fn bare_dot_lists_the_root_itself() {
    let (_tmp, root) = setup();
    let p = ConfinedPath::within(&root, Path::new(".")).unwrap();
    let entries = root.list_confined(&p, 64).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "src");
    assert_eq!(entries[0].kind, EntryKind::Dir);
}
