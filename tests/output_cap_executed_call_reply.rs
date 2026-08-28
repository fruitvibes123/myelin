                                                                           
                                                                               
                                                                              
                                                                       

use std::sync::Mutex;

use myelin::caps::{CallCaps, SessionBudget, StopReason};
use myelin::confine::Root;
use myelin::inference::{ChatMsg, ChatResponse, InferError, Inference, ToolCallRequest};
use myelin::tool_loop::{Decision, LoopApp, LoopOutcome, run};
use myelin::tools::{ToolBounds, ToolSpec, Toolbox};

struct OneRead {
    seen: Mutex<Vec<Vec<ChatMsg>>>,
    n: Mutex<usize>,
}

impl Inference for OneRead {
    fn model_id(&self) -> &str {
        "probe"
    }
    fn chat(&self, messages: &[ChatMsg], _t: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        self.seen.lock().unwrap().push(messages.to_vec());
        let mut n = self.n.lock().unwrap();
        *n += 1;
        if *n == 1 {
            Ok(ChatResponse {
                content: None,
                tool_calls: vec![ToolCallRequest {
                    id: "the-only-call".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"secret.txt"}"#.into(),
                }],
                length_capped: false,
            })
        } else {
            Ok(ChatResponse::done("wrapped up"))
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
fn output_cap_tells_the_model_the_result_was_dropped_not_that_it_did_not_run() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("secret.txt"), "the file content\n").unwrap();
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
        max_tool_calls: 10,
        wall_clock_ms: 30_000,
                                                                                                
        max_output_bytes: 1,
    };
    let session = SessionBudget::new(&caps, 8);
    let backend = OneRead {
        seen: Mutex::new(Vec::new()),
        n: Mutex::new(0),
    };
    let out = run(&backend, &session, caps, &tb, App);

    assert_eq!(out.stop_reason, StopReason::CapOutput);
                                                                                    
    assert_eq!(out.tool_calls, 1);
    assert_eq!(out.files_opened, vec!["secret.txt".to_string()]);

    let last = backend.seen.lock().unwrap().last().unwrap().clone();
    let reply = last
        .iter()
        .find_map(|m| match m {
            ChatMsg::Tool { call_id, content } if call_id == "the-only-call" => {
                Some(content.clone())
            }
            _ => None,
        })
        .expect("the executed call must have a tool result");
    assert_ne!(
        reply, "not executed: cap reached",
        "the loop answered an EXECUTED call with the not-executed refusal; \
         files_opened simultaneously reports {:?}",
        out.files_opened
    );
    assert!(
        reply.contains("dropped"),
        "the executed call's reply must say the result was dropped, got {reply:?}"
    );
}
