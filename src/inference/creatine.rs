                                                                                                      
                                                                                     
                                                                          
   
                                                                                                  
                                                                    
                                                                                                      
                                                                                                    
                                                                                              
                                                                                                        
                                                                                   
   
                                                                                                
                                                                                                       
                        

use creatine::caps::{RequestBudget, RequestCaps};
use creatine::engine::{Engine, EngineError, SessionKey};
use creatine::wire;

use crate::inference::{ChatMsg, ChatResponse, InferError, Inference, ToolCallRequest};
use crate::tools::ToolSpec;

                                                                                                        
                                                                                                       
                                                                                                   
                                                                                                    
                                                                                                 
                                                           
pub struct CreatineEngine<'e> {
    engine: &'e dyn Engine,
    model: String,
    key: Option<String>,
    caps: RequestCaps,
}

impl<'e> CreatineEngine<'e> {
                                                                                             
                                                                                          
                                    
    pub fn new(
        engine: &'e dyn Engine,
        model: impl Into<String>,
        key: Option<String>,
        caps: RequestCaps,
    ) -> CreatineEngine<'e> {
        CreatineEngine {
            engine,
            model: model.into(),
            key,
            caps,
        }
    }
}

impl Inference for CreatineEngine<'_> {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn chat(&self, messages: &[ChatMsg], tools: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        let req = build_request(&self.model, messages, tools);
                                                                                                 
        let session = match self.key.as_deref() {
            Some(k) => SessionKey::new(k),
            None => SessionKey::none(),
        };
                                                                                                     
                                        
        let mut budget = RequestBudget::new(self.caps);
        match self.engine.chat(&req, session, &mut budget) {
            Ok(resp) => translate_response(resp),
                                                                                                        
                                                                                                       
                                                                                                
                                                                                                         
                                                                   
                                                
            Err(EngineError::Budget(_)) => Err(InferError::Budget),
                                                                                                     
                                                              
            Err(e) => Err(InferError::Http(e.to_string())),
        }
    }
}

                                                                                           
                                                                                                  
fn build_request(model: &str, messages: &[ChatMsg], tools: &[ToolSpec]) -> wire::ChatRequest {
    let messages = messages.iter().map(to_wire_message).collect();
    let tools = if tools.is_empty() {
        None
    } else {
        Some(tools.iter().map(to_wire_tool).collect())
    };
    wire::ChatRequest {
        model: model.to_string(),
        messages,
        tools,
        max_tokens: None,
        temperature: None,
    }
}

fn to_wire_message(msg: &ChatMsg) -> wire::Message {
    match msg {
        ChatMsg::System(s) => wire::Message {
            role: wire::Role::System,
            content: Some(s.clone()),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMsg::User(s) => wire::Message {
            role: wire::Role::User,
            content: Some(s.clone()),
            tool_calls: None,
            tool_call_id: None,
        },
        ChatMsg::Assistant {
            content,
            tool_calls,
        } => wire::Message {
            role: wire::Role::Assistant,
            content: content.clone(),
                                                                                                       
                                                           
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.iter().map(to_wire_tool_call).collect())
            },
            tool_call_id: None,
        },
        ChatMsg::Tool { call_id, content } => wire::Message {
            role: wire::Role::Tool,
            content: Some(content.clone()),
            tool_calls: None,
            tool_call_id: Some(call_id.clone()),
        },
    }
}

fn to_wire_tool_call(c: &ToolCallRequest) -> wire::ToolCall {
    wire::ToolCall {
        id: c.id.clone(),
        kind: "function".to_string(),
        function: wire::FunctionCall {
            name: c.name.clone(),
            arguments: c.arguments.clone(),
        },
    }
}

fn to_wire_tool(t: &ToolSpec) -> wire::Tool {
    wire::Tool {
        kind: "function".to_string(),
        function: wire::FunctionDef {
            name: t.name.to_string(),
            description: Some(t.description.to_string()),
                                                                                             
                                                                                                
                      
            parameters: Some(t.parameters.to_string()),
        },
    }
}

                                                                               
                                                                                        
fn translate_response(resp: wire::ChatResponse) -> Result<ChatResponse, InferError> {
    let choice = resp
        .choices
        .into_iter()
        .next()
        .ok_or(InferError::BadResponse)?;
    let length_capped = choice.finish_reason == "length";
    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .map(|tc| ToolCallRequest {
            id: tc.id,
            name: tc.function.name,
            arguments: tc.function.arguments,                                                          
        })
        .collect();
    Ok(ChatResponse {
        content: choice.message.content,
        tool_calls,
        length_capped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

                      

    fn rcaps(max_gen: u32, max_ctx: u32) -> RequestCaps {
        RequestCaps {
            wall_clock: Duration::from_secs(30),
            max_body_bytes: 1 << 20,
            max_context_tokens: max_ctx,
            max_attn_bytes: 4 << 30,
            max_gen_tokens: max_gen,
                                                                                                         
                                                                      
            max_tool_calls: 16,
            max_tool_arg_bytes: 16384,
        }
    }

    fn cresp(
        model: &str,
        content: Option<&str>,
        finish: &str,
        tool_calls: Option<Vec<wire::ToolCall>>,
    ) -> wire::ChatResponse {
        wire::ChatResponse {
            id: "id".into(),
            object: "chat.completion".into(),
            created: 0,
            model: model.into(),
            choices: vec![wire::Choice {
                index: 0,
                message: wire::Message {
                    role: wire::Role::Assistant,
                    content: content.map(str::to_string),
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: finish.into(),
            }],
            usage: wire::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        }
    }

                                                                                                 
                                                                                                 
    struct RecordingEngine {
        keys: std::sync::Mutex<Vec<Option<String>>>,
    }
    impl RecordingEngine {
        fn new() -> RecordingEngine {
            RecordingEngine {
                keys: std::sync::Mutex::new(Vec::new()),
            }
        }
        fn keys(&self) -> Vec<Option<String>> {
            self.keys.lock().unwrap().clone()
        }
    }
    impl Engine for RecordingEngine {
        fn chat(
            &self,
            req: &wire::ChatRequest,
            session: SessionKey<'_>,
            _budget: &mut RequestBudget,
        ) -> Result<wire::ChatResponse, EngineError> {
            self.keys
                .lock()
                .unwrap()
                .push(session.get().map(str::to_string));
            Ok(cresp(&req.model, Some("ok"), "stop", None))
        }
        fn models(&self) -> Vec<wire::ModelInfo> {
            Vec::new()
        }
    }

                                                                                    

    #[test]
    fn translate_response_maps_tool_calls_length_and_empty() {
                                                                                    
        let resp = cresp(
            "m",
            None,
            "tool_calls",
            Some(vec![wire::ToolCall {
                id: "c1".into(),
                kind: "function".into(),
                function: wire::FunctionCall {
                    name: "read_file".into(),
                    arguments: "{\"path\":\"a\"}".into(),
                },
            }]),
        );
        let out = translate_response(resp).unwrap();
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "read_file");
        assert_eq!(out.tool_calls[0].id, "c1");
        assert_eq!(out.tool_calls[0].arguments, "{\"path\":\"a\"}");
        assert!(!out.length_capped);

                                                    
        assert!(
            translate_response(cresp("m", Some("partial"), "length", None))
                .unwrap()
                .length_capped
        );

                                                                   
        let empty = wire::ChatResponse {
            id: "x".into(),
            object: "chat.completion".into(),
            created: 0,
            model: "m".into(),
            choices: Vec::new(),
            usage: wire::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        };
        assert!(matches!(
            translate_response(empty),
            Err(InferError::BadResponse)
        ));
    }

    #[test]
    fn build_request_translates_all_message_variants_and_tools() {
        let msgs = vec![
            ChatMsg::System("sys".into()),
            ChatMsg::User("usr".into()),
            ChatMsg::Assistant {
                content: Some("a".into()),
                tool_calls: vec![ToolCallRequest {
                    id: "c1".into(),
                    name: "read_file".into(),
                    arguments: "{}".into(),
                }],
            },
            ChatMsg::Tool {
                call_id: "c1".into(),
                content: "result".into(),
            },
                                                                            
            ChatMsg::Assistant {
                content: None,
                tool_calls: vec![],
            },
        ];
        let tools = [ToolSpec {
            name: "read_file",
            description: "read a file",
            parameters: serde_json::json!({ "type": "object" }),
        }];
        let req = build_request("the-model", &msgs, &tools);

        assert_eq!(req.model, "the-model");
        assert_eq!(req.messages.len(), 5);
        assert!(matches!(req.messages[0].role, wire::Role::System));
        assert_eq!(req.messages[0].content.as_deref(), Some("sys"));
        assert!(req.messages[0].tool_calls.is_none());
        assert!(matches!(req.messages[1].role, wire::Role::User));
                                                                        
        let a = &req.messages[2];
        assert!(matches!(a.role, wire::Role::Assistant));
        let tc = a.tool_calls.as_ref().unwrap();
        assert_eq!(tc[0].function.name, "read_file");
        assert_eq!(tc[0].kind, "function");
                                                        
        let t = &req.messages[3];
        assert!(matches!(t.role, wire::Role::Tool));
        assert_eq!(t.tool_call_id.as_deref(), Some("c1"));
        assert_eq!(t.content.as_deref(), Some("result"));
                                                       
        assert!(req.messages[4].tool_calls.is_none());
                                                                   
        let offered = req.tools.as_ref().unwrap();
        assert_eq!(offered[0].function.name, "read_file");
        assert_eq!(
            offered[0].function.description.as_deref(),
            Some("read a file")
        );
        assert!(
            offered[0]
                .function
                .parameters
                .as_deref()
                .unwrap()
                .contains("object")
        );
                                                              
        assert!(req.max_tokens.is_none());
                                
        assert!(build_request("m", &msgs, &[]).tools.is_none());
    }

                                                                                          

    #[test]
    fn session_key_passed_per_turn_same_job_and_distinct_per_job() {
        let rec = RecordingEngine::new();
        let caps = rcaps(100, 10_000);
                                                                                                          
        let job1 = CreatineEngine::new(&rec, "m0", Some("job-1".into()), caps);
        let _ = job1.chat(&[ChatMsg::User("a".into())], &[]);
        let _ = job1.chat(&[ChatMsg::User("b".into())], &[]);
        drop(job1);
                                                                                      
        let job2 = CreatineEngine::new(&rec, "m0", Some("job-2".into()), caps);
        let _ = job2.chat(&[ChatMsg::User("c".into())], &[]);
        drop(job2);
                                                                 
        let keyless = CreatineEngine::new(&rec, "m0", None, caps);
        let _ = keyless.chat(&[ChatMsg::User("d".into())], &[]);
        assert_eq!(
            rec.keys(),
            vec![
                Some("job-1".to_string()),
                Some("job-1".to_string()),
                Some("job-2".to_string()),
                None,
            ]
        );
    }
}
