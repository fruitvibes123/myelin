                                                                                 
                                                 

use std::sync::Mutex;

use myelin::caps::{CallCaps, SessionBudget};
use myelin::confine::Root;
use myelin::inference::{ChatMsg, ChatResponse, InferError, Inference, ToolCallRequest};
use myelin::tool_loop::{Decision, LoopApp, LoopOutcome, run};
use myelin::tools::{ToolBounds, ToolSpec, Toolbox};

struct Scripted {
    n: Mutex<usize>,
}

impl Inference for Scripted {
    fn model_id(&self) -> &str {
        "scr"
    }
    fn chat(&self, _messages: &[ChatMsg], _tools: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        let mut n = self.n.lock().unwrap();
        *n += 1;
        if *n == 1 {
                                                                    
            let calls = ["src/lib.rs", "./src/lib.rs", "src/./lib.rs"]
                .iter()
                .enumerate()
                .map(|(k, p)| ToolCallRequest {
                    id: format!("r{k}"),
                    name: "read_file".into(),
                    arguments: format!(r#"{{"path":"{p}"}}"#),
                })
                .collect();
            Ok(ChatResponse {
                content: None,
                tool_calls: calls,
                length_capped: false,
            })
        } else {
            Ok(ChatResponse {
                content: Some("done".into()),
                tool_calls: Vec::new(),
                length_capped: false,
            })
        }
    }
}

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
    fn assemble(self, o: LoopOutcome) -> LoopOutcome {
        o
    }
}

#[test]
fn three_spellings_of_one_file_dedup_to_one_entry() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    let caps = CallCaps {
        max_iterations: 10,
        max_tool_calls: 100,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let session = SessionBudget::new(&caps, 8);
    let out = run(&Scripted { n: Mutex::new(0) }, &session, caps, &tb, App);
    assert_eq!(
        out.files_opened,
        vec!["src/lib.rs".to_string()],
        "files_opened should dedup normalized spellings, got {:?}",
        out.files_opened
    );
}
