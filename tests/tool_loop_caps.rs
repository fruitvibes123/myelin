                                                                             
                                                                              
                                                           

use myelin::caps::{CallCaps, SessionBudget, StopReason};
use myelin::confine::Root;
use myelin::inference::{ChatMsg, MockInference};
use myelin::tool_loop::{Decision, LoopApp, LoopOutcome, run};
use myelin::tools::{ToolBounds, Toolbox};

struct App;
impl LoopApp for App {
    type Product = LoopOutcome;
    fn initial_messages(&self) -> Vec<ChatMsg> {
        vec![ChatMsg::User("go".into())]
    }
    fn on_text_turn(&mut self, _t: &str) -> Decision {
        Decision::Stop
    }
    fn wrap_up_prompt(&self) -> String {
        "wrap up".into()
    }
    fn assemble(self, outcome: LoopOutcome) -> LoopOutcome {
        outcome
    }
}

fn fixture() -> (tempfile::TempDir, Root) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    (tmp, root)
}

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
fn wallclock_cap_makes_no_further_model_call() {
    let (_tmp, root) = fixture();
                                                                                 
                                                                                  
                                  
    let backend = MockInference::slow_explorer(120, "salvaged");
    let caps = CallCaps {
        max_iterations: 100,
        max_tool_calls: 100,
        wall_clock_ms: 200,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 8);
    let out = run(&backend, &session, caps, &toolbox(&root), App);

    assert_eq!(out.stop_reason, StopReason::CapWallclock);
    assert_eq!(
        backend.calls(),
        2,
        "the salvage turn ran after the wall-clock cap, overrunning the deadline"
    );
    assert!(
        out.final_text.is_none(),
        "the salvage produced text after the wall-clock budget was spent"
    );
}

#[test]
fn iterations_cap_still_salvages() {
    let (_tmp, root) = fixture();
                                                                                   
                                                                                    
    let backend = MockInference::slow_explorer(0, "salvaged");
    let caps = CallCaps {
        max_iterations: 2,
        max_tool_calls: 100,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 8);
    let out = run(&backend, &session, caps, &toolbox(&root), App);

    assert_eq!(out.stop_reason, StopReason::CapIterations);
    assert_eq!(
        out.final_text.as_deref(),
        Some("salvaged"),
        "the iterations-cap salvage turn must still run"
    );
    assert_eq!(
        backend.calls(),
        3,
        "two explore turns plus one salvage turn"
    );
}
