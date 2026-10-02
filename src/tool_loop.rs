                                                         
   
                                                                               
                                                                             
                                                                               
                                                                               
                                                                                
                                                                                 
                   
   
                                                                                     
                                                                              
                                                                                 
                                                                                   
                                                      
   
                                                                       
                                                                              
                                                                                                   
                                                                                 
                                                                                       
                                                     

use crate::caps::{CallBudget, CallCaps, SessionBudget, StopReason};
use crate::inference::{ChatMsg, InferError, Inference, ToolCallRequest};
use crate::tools::{ToolCall, ToolSpec, Toolbox};

                                                                          
#[derive(Debug, Clone)]
pub enum Decision {
                                                                            
                                                                                 
                                                   
    Stop,
                                                                         
                                                                                  
                                        
    Continue(String),
}

                                                                           
                                                                                 
                                                                              
                                                   
   
                                                                                
                                                                                  
                                                                            
                                                                           
                                                                              
                                                                                
                                                                                                
                                                                                
                    
#[derive(Debug, Clone)]
pub struct LoopOutcome {
    pub final_text: Option<String>,
    pub model: String,
    pub iterations: u32,
    pub tool_calls: u32,
    pub wall_clock_ms: u64,
    pub stop_reason: StopReason,
                                                                                  
                                                               
    pub last_error: Option<String>,
                                                                              
                                                                               
                                                                                   
                                                                         
                                                                               
                                                                                  
                                                                                  
    pub terminal_turn: bool,
                                                               
                                                                             
                                  
    pub length_capped: bool,
                                                                                       
                                                                                   
                                                                                      
                                                                                      
                                                                                        
                                                                                      
                                          
    pub backend_budget: bool,
                                                                                  
                                                                                  
                                                                               
                                                                                   
                                                                                                                       
                                                                                  
                                                                                  
                                                                               
                                                    
    pub files_opened: Vec<String>,
}

                                                                                
                                                                              
                                                                             
                                                                         
pub trait LoopApp {
                                                                                                           
    type Product;

                                                                                 
    fn initial_messages(&self) -> Vec<ChatMsg>;

                                                                                
                                                                             
                                                                                                  
                                                      
    fn on_text_turn(&mut self, text: &str) -> Decision;

                                                                          
                                                                                                
                                                                                     
    fn wrap_up_prompt(&self) -> String;

                                                                             
                                                                           
                                                                                 
                                                                                                 
                                                                               
                                                                               
    fn soft_deadline(&self, _hard_ms: u64) -> Option<u64> {
        None
    }

                                                                                  
                                                                                               
                                                                              
    fn on_wind_down(&mut self) {}

                                                                                
                                                                                                   
                                        
    fn assemble(self, outcome: LoopOutcome) -> Self::Product;
}

                                                                                 
                                                                                  
                                                                                 
                    
pub fn run<A: LoopApp>(
    inference: &dyn Inference,
    session: &SessionBudget,
    caps: CallCaps,
    toolbox: &Toolbox<'_>,
    mut app: A,
) -> A::Product {
    let mut budget = CallBudget::new(caps, session);
    let specs = ToolCall::specs(toolbox.git_enabled, toolbox.search.is_some());
    let mut messages = app.initial_messages();
    let mut final_text: Option<String> = None;
    let mut last_error: Option<String> = None;
                                                                            
                                                                                   
                                                                               
                                                         
    let soft_deadline = app.soft_deadline(caps.wall_clock_ms);
    let mut wrap_up = false;
    let mut terminal_turn = false;
    let mut term_length_capped = false;
                                                                                         
                                                                                      
    let mut backend_budget = false;
                                                                        
                                                                            
    let mut files_opened: Vec<String> = Vec::new();
                                                                                
                                                                               
                                                                            
                                         
    let mut dispatched: u32 = 0;

    let stop = 'outer: loop {
        if let Err(reason) = budget.begin_iteration() {
            break reason;
        }
                                                                                   
                                                                              
                                                            
        if !wrap_up
            && let Some(sd) = soft_deadline
            && budget.elapsed_ms() >= sd
        {
            wrap_up = true;
            app.on_wind_down();
            messages.push(ChatMsg::User(app.wrap_up_prompt()));
        }
        let offered: &[ToolSpec] = if wrap_up { &[] } else { &specs };
        let response = match inference.chat(&messages, offered) {
            Ok(r) => r,
            Err(e) => {
                                                                                         
                                                                                               
                backend_budget = matches!(e, InferError::Budget);
                last_error = Some(e.to_string());
                break StopReason::Error;
            }
        };

        if response.tool_calls.is_empty() {
                                                                                      
            let this_capped = response.length_capped();
            let text = response.content.unwrap_or_default();
                                                                                 
                                                           
            final_text = Some(text.clone());
            match app.on_text_turn(&text) {
                Decision::Continue(follow) => {
                    messages.push(ChatMsg::Assistant {
                        content: Some(text),
                        tool_calls: Vec::new(),
                    });
                    messages.push(ChatMsg::User(follow));
                    continue;
                }
                                                                             
                                                                                    
                                                                                 
                          
                Decision::Stop => {
                    terminal_turn = true;
                    term_length_capped = this_capped;
                    break if wrap_up {
                        StopReason::CapWallclock
                    } else {
                        StopReason::Done
                    };
                }
            }
        }

                                                                                  
                                                                                   
                                                                        
        if wrap_up {
            messages.push(ChatMsg::Assistant {
                content: response.content.clone(),
                tool_calls: response.tool_calls.clone(),
            });
            refuse_pending(
                &mut messages,
                &response.tool_calls,
                "not executed: winding down",
            );
            messages.push(ChatMsg::User(app.wrap_up_prompt()));
            continue;
        }

        messages.push(ChatMsg::Assistant {
            content: response.content.clone(),
            tool_calls: response.tool_calls.clone(),
        });
        for (idx, call) in response.tool_calls.iter().enumerate() {
            if let Err(reason) = budget.charge_tool_call() {
                refuse_pending(
                    &mut messages,
                    &response.tool_calls[idx..],
                    "not executed: cap reached",
                );
                break 'outer reason;
            }
            let (result, opened) = execute(toolbox, call);
            dispatched += 1;
            if let Some(path) = opened
                && !files_opened.contains(&path)
            {
                files_opened.push(path);
            }
            if let Err(reason) = budget.charge_output(result.len() as u64) {
                                                                              
                                                                               
                                                                           
                messages.push(ChatMsg::Tool {
                    call_id: call.id.clone(),
                    content: "result dropped: output cap reached".to_string(),
                });
                refuse_pending(
                    &mut messages,
                    &response.tool_calls[idx + 1..],
                    "not executed: cap reached",
                );
                break 'outer reason;
            }
            messages.push(ChatMsg::Tool {
                call_id: call.id.clone(),
                content: result,
            });
        }
    };

                                                                                  
                                                                           
                                                                                   
                                                                                    
                                                                                  
                                                                               
                                                                            
                                                                                    
                                                                                    
                                                                                  
                                                
    if final_text.is_none()
        && dispatched > 0
        && matches!(
            stop,
            StopReason::CapIterations | StopReason::CapToolCalls | StopReason::CapOutput
        )
    {
        messages.push(ChatMsg::User(app.wrap_up_prompt()));
        if let Ok(r) = inference.chat(&messages, &[])
            && let Some(t) = r.content
            && !t.trim().is_empty()
        {
            final_text = Some(t);
        }
    }

    app.assemble(LoopOutcome {
        final_text,
        model: inference.model_id().to_string(),
        iterations: budget.iterations(),
        tool_calls: dispatched,
        wall_clock_ms: budget.elapsed_ms(),
        stop_reason: stop,
        last_error,
        terminal_turn,
        length_capped: term_length_capped,
        backend_budget,
        files_opened,
    })
}

                                                                               
                                                                            
                                                                                 
                   
fn refuse_pending(messages: &mut Vec<ChatMsg>, calls: &[ToolCallRequest], reason: &str) {
    for call in calls {
        messages.push(ChatMsg::Tool {
            call_id: call.id.clone(),
            content: reason.to_string(),
        });
    }
}

                                                                                  
                                                                                   
                                                                                  
                                                                              
                                                                                    
                          
fn execute(toolbox: &Toolbox<'_>, call: &ToolCallRequest) -> (String, Option<String>) {
    match ToolCall::parse(&call.name, &call.arguments) {
        Ok(tc) => match toolbox.dispatch(&tc) {
            Ok(text) => {
                                                                               
                                                                             
                let opened = match &tc {
                    ToolCall::ReadFile { path, .. } => toolbox
                        .confined(&crate::confine::RelPath::normalize(path))
                        .ok()
                        .map(|c| c.rel().to_string_lossy().into_owned()),
                    _ => None,
                };
                (text, opened)
            }
            Err(e) => (format!("tool error: {e}"), None),
        },
        Err(e) => (format!("tool error: {e}"), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confine::Root;
    use crate::inference::{ChatResponse, InferError};
    use crate::tools::ToolBounds;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

                                                                               
                                                                                   
                                                                 
    struct ScriptMock {
        script: Vec<ChatResponse>,
        idx: AtomicUsize,
        offered: Mutex<Vec<usize>>,
    }

    impl ScriptMock {
        fn new(script: Vec<ChatResponse>) -> ScriptMock {
            ScriptMock {
                script,
                idx: AtomicUsize::new(0),
                offered: Mutex::new(Vec::new()),
            }
        }
        fn offered_lens(&self) -> Vec<usize> {
            self.offered.lock().unwrap().clone()
        }
    }

    impl Inference for ScriptMock {
        fn model_id(&self) -> &str {
            "mock"
        }
        fn chat(
            &self,
            _messages: &[ChatMsg],
            tools: &[ToolSpec],
        ) -> Result<ChatResponse, InferError> {
            let i = self.idx.fetch_add(1, Ordering::SeqCst);
            self.offered.lock().unwrap().push(tools.len());
            self.script
                .get(i)
                .cloned()
                .ok_or(InferError::ScriptExhausted)
        }
    }

                                                                                  
                                                                           
    struct BudgetErrMock;
    impl Inference for BudgetErrMock {
        fn model_id(&self) -> &str {
            "mock"
        }
        fn chat(
            &self,
            _messages: &[ChatMsg],
            _tools: &[ToolSpec],
        ) -> Result<ChatResponse, InferError> {
            Err(InferError::Budget)
        }
    }

                                                                                    
                                                                           
    struct TestApp {
        initial: Vec<ChatMsg>,
        soft: Option<u64>,
        continues: u32,
        wound_down: bool,
    }

    impl TestApp {
        fn new() -> TestApp {
            TestApp {
                initial: vec![ChatMsg::System("s".into()), ChatMsg::User("go".into())],
                soft: None,
                continues: 0,
                wound_down: false,
            }
        }
    }

    impl LoopApp for TestApp {
        type Product = (LoopOutcome, bool);
        fn initial_messages(&self) -> Vec<ChatMsg> {
            self.initial.clone()
        }
        fn on_text_turn(&mut self, _text: &str) -> Decision {
            if self.continues > 0 {
                self.continues -= 1;
                Decision::Continue("retry".into())
            } else {
                Decision::Stop
            }
        }
        fn wrap_up_prompt(&self) -> String {
            "wrap up now".into()
        }
        fn soft_deadline(&self, _hard_ms: u64) -> Option<u64> {
            self.soft
        }
        fn on_wind_down(&mut self) {
            self.wound_down = true;
        }
        fn assemble(self, outcome: LoopOutcome) -> (LoopOutcome, bool) {
            (outcome, self.wound_down)
        }
    }

    fn temp_repo() -> (tempfile::TempDir, Root) {
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

    fn caps(max_iterations: u32) -> CallCaps {
        CallCaps {
            max_iterations,
            max_tool_calls: 32,
            wall_clock_ms: 30_000,
            max_output_bytes: 1 << 20,
        }
    }

    #[test]
    fn terminal_text_turn_assembles_done() {
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![ChatResponse::done("the report")]);
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let (out, wound) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::Done);
        assert_eq!(out.final_text.as_deref(), Some("the report"));
        assert_eq!(out.iterations, 1);
        assert_eq!(out.tool_calls, 0);
        assert_eq!(out.model, "mock");
        assert!(out.terminal_turn, "ended on a terminal Stop");
        assert!(!wound);
                                               
        assert_eq!(mock.offered_lens().len(), 1);
        assert!(mock.offered_lens()[0] > 0, "tools offered on a normal turn");
    }

    #[test]
    fn tool_call_then_text_dispatches_and_feeds_back() {
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![
            ChatResponse::tool("list_dir", serde_json::json!({ "path": "." })),
            ChatResponse::done("done"),
        ]);
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::Done);
        assert_eq!(out.tool_calls, 1);
        assert_eq!(out.iterations, 2);
        assert_eq!(out.final_text.as_deref(), Some("done"));
    }

    #[test]
    fn tool_error_is_fed_back_not_a_loop_failure() {
                                                                            
                     
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![
            ChatResponse::tool(
                "read_file",
                serde_json::json!({ "path": "../../etc/passwd" }),
            ),
            ChatResponse::done("done"),
        ]);
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::Done);
        assert_eq!(out.tool_calls, 1);
        assert_eq!(out.final_text.as_deref(), Some("done"));
    }

    #[test]
    fn continue_loops_with_the_followup_then_stops() {
                                                                            
                      
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![
            ChatResponse::done("draft"),
            ChatResponse::done("final"),
        ]);
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let mut app = TestApp::new();
        app.continues = 1;
        let (out, _) = run(&mock, &session, c, &toolbox(&root), app);
        assert_eq!(out.stop_reason, StopReason::Done);
        assert_eq!(out.iterations, 2);
        assert_eq!(out.final_text.as_deref(), Some("final"));
    }

    #[test]
    fn iteration_cap_with_no_work_yields_no_product() {
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![]);
        let c = caps(0);                                           
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::CapIterations);
        assert!(
            !out.terminal_turn,
            "a begin_iteration cap break is not a terminal turn"
        );
        assert_eq!(out.final_text, None);
        assert_eq!(out.tool_calls, 0);
                                                                                
                                                                                  
        assert_eq!(out.iterations, 1);
    }

    #[test]
    fn cap_after_tool_use_salvages_a_product() {
                                                                                
                                                                       
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![
            ChatResponse::tool("list_dir", serde_json::json!({ "path": "." })),
            ChatResponse::tool("list_dir", serde_json::json!({ "path": "." })),
            ChatResponse::done("salvaged"),
        ]);
        let c = caps(2);                                                 
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::CapIterations);
        assert_eq!(out.final_text.as_deref(), Some("salvaged"));
        assert_eq!(out.tool_calls, 2);
                                                                              
                                                                               
        assert_eq!(out.iterations, 3);
                                                
        assert_eq!(mock.offered_lens().last(), Some(&0));
                                                                                 
        assert!(!out.terminal_turn);
    }

    #[test]
    fn soft_deadline_winds_down_no_tools_and_caps_wallclock() {
                                                                              
                                                                                    
                                                                                 
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![ChatResponse::done("the report")]);
        let c = CallCaps {
            max_iterations: 10,
            max_tool_calls: 32,
            wall_clock_ms: 60_000,
            max_output_bytes: 1 << 20,
        };
        let session = SessionBudget::new(&c, 8);
        let mut app = TestApp::new();
        app.soft = Some(0);
        let (out, wound) = run(&mock, &session, c, &toolbox(&root), app);
        assert!(wound, "on_wind_down fired");
        assert_eq!(out.stop_reason, StopReason::CapWallclock);
        assert!(
            out.terminal_turn,
            "the model answered in wind-down: a terminal Stop even though the reason is CapWallclock"
        );
        assert_eq!(out.final_text.as_deref(), Some("the report"));
        assert_eq!(
            mock.offered_lens(),
            vec![0],
            "no tools offered in wind-down"
        );
    }

    #[test]
    fn backend_error_reaches_assemble_without_panic() {
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![]);                                  
        let c = caps(10);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::Error);
        assert_eq!(out.final_text, None);
        assert_eq!(out.tool_calls, 0);
        assert!(!out.terminal_turn);
                                                                       
        assert!(out.last_error.is_some_and(|e| e.contains("exhausted")));
    }

    #[test]
    fn backend_budget_set_on_inferror_budget() {
                                                                                  
                                                                              
                                 
        let (_tmp, root) = temp_repo();
        let c = caps(10);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&BudgetErrMock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.stop_reason, StopReason::Error);
        assert!(out.backend_budget, "InferError::Budget sets backend_budget");
        assert_eq!(out.final_text, None);
        assert!(!out.terminal_turn);

                                                                      
        let session2 = SessionBudget::new(&c, 8);
        let (out2, _) = run(
            &ScriptMock::new(vec![]),
            &session2,
            c,
            &toolbox(&root),
            TestApp::new(),
        );
        assert_eq!(out2.stop_reason, StopReason::Error);
        assert!(!out2.backend_budget);
    }

    #[test]
    fn terminal_length_cap_surfaces_on_outcome() {
                                                                               
                                                                          
        let (_tmp, root) = temp_repo();
        let capped = ChatResponse {
            content: Some("partial".into()),
            tool_calls: Vec::new(),
            finish_reason: Some("length".into()),
        };
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(
            &ScriptMock::new(vec![capped]),
            &session,
            c,
            &toolbox(&root),
            TestApp::new(),
        );
        assert!(out.length_capped);
        assert!(out.terminal_turn);

        let session2 = SessionBudget::new(&c, 8);
        let (out2, _) = run(
            &ScriptMock::new(vec![ChatResponse::done("done")]),
            &session2,
            c,
            &toolbox(&root),
            TestApp::new(),
        );
        assert!(!out2.length_capped);
    }

    #[test]
    fn files_opened_records_in_scope_reads_only() {
                                                                                 
                                                                                    
                                                              
        let (_tmp, root) = temp_repo();
        let mock = ScriptMock::new(vec![
            ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
            ChatResponse::tool("read_file", serde_json::json!({ "path": "src/lib.rs" })),
            ChatResponse::tool("list_dir", serde_json::json!({ "path": "." })),
            ChatResponse::tool(
                "read_file",
                serde_json::json!({ "path": "../../etc/passwd" }),
            ),
            ChatResponse::done("done"),
        ]);
        let c = caps(8);
        let session = SessionBudget::new(&c, 8);
        let (out, _) = run(&mock, &session, c, &toolbox(&root), TestApp::new());
        assert_eq!(out.files_opened, vec!["src/lib.rs".to_string()]);
    }
}
