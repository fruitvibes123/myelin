                                                                               
                                                                                   

use std::collections::HashSet;
use std::sync::Mutex;

use myelin::caps::{CallCaps, SessionBudget, StopReason};
use myelin::confine::Root;
use myelin::inference::{ChatMsg, ChatResponse, InferError, Inference, ToolCallRequest};
use myelin::tool_loop::{Decision, LoopApp, LoopOutcome, run};
use myelin::tools::{ToolBounds, ToolSpec, Toolbox};

                                                                            
                                                                               
                                           
struct Recorder {
    transcripts: Mutex<Vec<Vec<ChatMsg>>>,
    n: Mutex<usize>,
    calls_per_turn: usize,
}

impl Inference for Recorder {
    fn model_id(&self) -> &str {
        "rec"
    }
    fn chat(&self, messages: &[ChatMsg], _tools: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        self.transcripts.lock().unwrap().push(messages.to_vec());
        let mut n = self.n.lock().unwrap();
        *n += 1;
        let tool_calls = (0..self.calls_per_turn)
            .map(|k| ToolCallRequest {
                id: format!("c{}_{k}", *n),
                name: "list_dir".into(),
                arguments: r#"{"path":"."}"#.into(),
            })
            .collect();
        Ok(ChatResponse {
            content: None,
            tool_calls,
            finish_reason: None,
        })
    }
}

struct App {
    soft: Option<u64>,
}
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
    fn soft_deadline(&self, _hard: u64) -> Option<u64> {
        self.soft
    }
    fn assemble(self, o: LoopOutcome) -> LoopOutcome {
        o
    }
}

fn assert_every_chat_balanced(transcripts: &[Vec<ChatMsg>]) {
    for (i, msgs) in transcripts.iter().enumerate() {
        let answered: HashSet<&str> = msgs
            .iter()
            .filter_map(|m| match m {
                ChatMsg::Tool { call_id, .. } => Some(call_id.as_str()),
                _ => None,
            })
            .collect();
        for m in msgs {
            if let ChatMsg::Assistant { tool_calls, .. } = m {
                for c in tool_calls {
                    assert!(
                        answered.contains(c.id.as_str()),
                        "chat #{i}: assistant tool_call id {} has no matching tool result",
                        c.id
                    );
                }
            }
        }
    }
}

fn drive(
    caps: CallCaps,
    calls_per_turn: usize,
    soft: Option<u64>,
) -> (StopReason, Vec<Vec<ChatMsg>>) {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds::default(),
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    let rec = Recorder {
        transcripts: Mutex::new(Vec::new()),
        n: Mutex::new(0),
        calls_per_turn,
    };
    let session = SessionBudget::new(&caps, 8);
    let out = run(&rec, &session, caps, &tb, App { soft });
    let t = rec.transcripts.lock().unwrap().clone();
    (out.stop_reason, t)
}

#[test]
fn tool_call_cap_leaves_no_unanswered_id() {
    let caps = CallCaps {
        max_iterations: 100,
        max_tool_calls: 1,                                              
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let (stop, transcripts) = drive(caps, 2, None);
    assert_eq!(stop, StopReason::CapToolCalls);
    assert!(transcripts.len() >= 2, "no salvage chat happened");
    assert_every_chat_balanced(&transcripts);
}

#[test]
fn output_cap_leaves_no_unanswered_id() {
    let caps = CallCaps {
        max_iterations: 100,
        max_tool_calls: 100,
        wall_clock_ms: 30_000,
        max_output_bytes: 1,                                  
    };
    let (stop, transcripts) = drive(caps, 2, None);
    assert_eq!(stop, StopReason::CapOutput);
    assert!(transcripts.len() >= 2, "no salvage chat happened");
    assert_every_chat_balanced(&transcripts);
}

#[test]
fn winddown_leaves_no_unanswered_id() {
    let caps = CallCaps {
        max_iterations: 3,
        max_tool_calls: 100,
        wall_clock_ms: 30_000,
        max_output_bytes: 1 << 20,
    };
    let (_stop, transcripts) = drive(caps, 1, Some(0));
    assert!(transcripts.len() >= 2, "wind-down did not re-ask");
    assert_every_chat_balanced(&transcripts);
}
