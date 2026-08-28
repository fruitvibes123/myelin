                                                                                 

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use myelin::confine::{ConfineError, Root};
use myelin::tools::{ToolBounds, ToolCall, ToolError, Toolbox};

fn mkfifo(p: &Path) {
    let st = std::process::Command::new("mkfifo")
        .arg(p)
        .status()
        .expect("mkfifo");
    assert!(st.success(), "mkfifo failed");
}

                                                                             
                                                                               
                                             
fn dispatch_with_watchdog(dir: std::path::PathBuf, call: ToolCall) -> Result<String, ToolError> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let root = Root::open(&dir).unwrap();
        let tb = Toolbox {
            root: &root,
            bounds: ToolBounds::default(),
            scope: None,
            git_enabled: false,
            search: None,
            excluded_dirs: &[],
        };
        let _ = tx.send(tb.dispatch(&call));
    });
    rx.recv_timeout(Duration::from_secs(5))
        .expect("dispatch blocked past 5s on a FIFO in the confined root")
}

#[test]
fn read_file_refuses_a_fifo_without_blocking() {
    let tmp = tempfile::tempdir().unwrap();
    mkfifo(&tmp.path().join("pipe"));
    let r = dispatch_with_watchdog(
        tmp.path().to_path_buf(),
        ToolCall::ReadFile {
            path: "pipe".into(),
            range: None,
        },
    );
    assert!(
        matches!(r, Err(ToolError::Confine(ConfineError::NotRegularFile))),
        "read_file on a FIFO must be the non-regular-file refusal, got {r:?}"
    );
}

#[test]
fn grep_path_arg_refuses_a_fifo_without_blocking() {
    let tmp = tempfile::tempdir().unwrap();
    mkfifo(&tmp.path().join("pipe"));
    let r = dispatch_with_watchdog(
        tmp.path().to_path_buf(),
        ToolCall::Grep {
            pattern: "x".into(),
            path: Some("pipe".into()),
        },
    );
    assert!(
        r.is_err(),
        "grep with a FIFO path arg must refuse, not return content, got {r:?}"
    );
}
