                                                                              
                                                                               
                                                                               
                                                                                     

use myelin::caps::{CallCaps, SessionBudget, StopReason};
use myelin::confine::Root;
use myelin::inference::{ChatMsg, ChatResponse, MockInference};
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
fn dispatched_then_cap_salvages() {
    let (_tmp, root) = fixture();
    let caps = CallCaps {
        max_iterations: 1,
        max_tool_calls: 8,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 8);
    let backend = MockInference::scripted(vec![
        ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
        ChatResponse::done("salvaged"),
    ]);
    let out = run(&backend, &session, caps, &toolbox(&root), App);
    assert_eq!(out.stop_reason, StopReason::CapIterations);
    assert_eq!(out.tool_calls, 1);
    assert_eq!(out.files_opened, vec!["src/lib.rs".to_string()]);
    assert_eq!(backend.calls(), 2, "the second call is the salvage wrap-up");
    assert_eq!(out.final_text.as_deref(), Some("salvaged"));
}

                                                                                 
                                                                                           
#[test]
fn session_budget_spent_first_charge_refused_no_wrap_up() {
    let (_tmp, root) = fixture();
    let caps = CallCaps {
        max_iterations: 10,
        max_tool_calls: 1,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 1);                                

                                                 
    let b1 = MockInference::scripted(vec![
        ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
        ChatResponse::done("first"),
    ]);
    let o1 = run(&b1, &session, caps, &toolbox(&root), App);
    assert_eq!(o1.files_opened, vec!["src/lib.rs".to_string()]);
    assert_eq!(o1.tool_calls, 1);

                                                                                    
    let b2 = MockInference::scripted(vec![
        ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
        ChatResponse::done("salvaged"),
    ]);
    let o2 = run(&b2, &session, caps, &toolbox(&root), App);
    assert_eq!(o2.stop_reason, StopReason::CapToolCalls);
    assert_eq!(o2.tool_calls, 0, "nothing dispatched, so the count is zero");
    assert!(o2.files_opened.is_empty());
    assert_eq!(
        b2.calls(),
        1,
        "no salvage wrap-up: only the first model call ran"
    );
    assert_eq!(o2.final_text, None);
}

                                                                                   
                                  
#[test]
fn max_tool_calls_zero_no_wrap_up() {
    let (_tmp, root) = fixture();
    let caps = CallCaps {
        max_iterations: 10,
        max_tool_calls: 0,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 8);
    let backend = MockInference::scripted(vec![
        ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
        ChatResponse::done("salvaged"),
    ]);
    let out = run(&backend, &session, caps, &toolbox(&root), App);
    assert_eq!(out.stop_reason, StopReason::CapToolCalls);
    assert_eq!(out.tool_calls, 0);
    assert!(out.files_opened.is_empty());
    assert_eq!(backend.calls(), 1, "no salvage wrap-up");
    assert_eq!(out.final_text, None);
}
