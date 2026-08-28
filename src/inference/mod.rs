                                                           
                                                                           
                                                                   

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use crate::tools::ToolSpec;

#[cfg(feature = "net")]
pub mod lmstudio;

                                                                            
                                                                        
                                                                            
                                                                    
                                                                             
                                                                            
                                                                   
pub const MAX_MODEL_TIMEOUT_SECS: u64 = 86_400;

                                                                                 
                                                                                  
                                                 
#[cfg(feature = "creatine-inprocess")]
pub mod creatine;

#[cfg(feature = "net")]
pub use lmstudio::LmStudioInference;

#[cfg(feature = "creatine-inprocess")]
pub use self::creatine::CreatineEngine;

                                                                           
                               
#[derive(Debug, Clone)]
pub enum ChatMsg {
    System(String),
    User(String),
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCallRequest>,
    },
                                                                   
    Tool {
        call_id: String,
        content: String,
    },
}

                                                                              
                                                                
#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
                                                                                
                                                               
                                                                               
                                                                           
                                                  
    pub length_capped: bool,
}

impl ChatResponse {
                                             
    pub fn done(text: &str) -> ChatResponse {
        ChatResponse {
            content: Some(text.to_string()),
            tool_calls: Vec::new(),
            length_capped: false,
        }
    }

                                                 
    pub fn tool(name: &str, arguments: serde_json::Value) -> ChatResponse {
        ChatResponse {
            content: None,
            tool_calls: vec![ToolCallRequest {
                id: format!("call-{name}"),
                name: name.to_string(),
                arguments: arguments.to_string(),
            }],
            length_capped: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InferError {
    #[error("model endpoint error: {0}")]
    Http(String),
    #[error("model response was not the expected shape")]
    BadResponse,
    #[error("external model endpoint requires a key pin")]
    PinRequired,
    #[error("mock script exhausted")]
    ScriptExhausted,
                                                                                       
                                                                                       
                                                                                       
                                                                                         
    #[error("backend resource budget reached")]
    Budget,
    #[cfg(feature = "net")]
    #[error(transparent)]
    Tls(#[from] crate::tls::TlsError),
}

pub trait Inference: Send + Sync {
    fn model_id(&self) -> &str;
    fn chat(&self, messages: &[ChatMsg], tools: &[ToolSpec]) -> Result<ChatResponse, InferError>;
}

                                                                             
                                                                              
                                   
pub struct MockInference {
    script: Mutex<VecDeque<ChatResponse>>,
    repeat: Option<ChatResponse>,
    calls: Mutex<u32>,
                                                                           
    delay: Duration,
                                                                              
                                                                           
                                                                                 
                                                      
    wrap_up_report: Option<String>,
}

impl MockInference {
    pub fn scripted(steps: Vec<ChatResponse>) -> MockInference {
        MockInference {
            script: Mutex::new(steps.into()),
            repeat: None,
            calls: Mutex::new(0),
            delay: Duration::ZERO,
            wrap_up_report: None,
        }
    }

                                                                             
                                               
    pub fn never_stops() -> MockInference {
        MockInference {
            script: Mutex::new(VecDeque::new()),
            repeat: Some(ChatResponse::tool(
                "list_dir",
                serde_json::json!({ "path": "." }),
            )),
            calls: Mutex::new(0),
            delay: Duration::ZERO,
            wrap_up_report: None,
        }
    }

                                                                                  
                                                                                 
                                                                                 
                                                                            
    pub fn slow_explorer(delay_ms: u64, report: &str) -> MockInference {
        MockInference {
            script: Mutex::new(VecDeque::new()),
            repeat: None,
            calls: Mutex::new(0),
            delay: Duration::from_millis(delay_ms),
            wrap_up_report: Some(report.to_string()),
        }
    }

                                          
    pub fn calls(&self) -> u32 {
        self.calls.lock().map(|c| *c).unwrap_or(0)
    }
}

impl Inference for MockInference {
    fn model_id(&self) -> &str {
        "mock"
    }

    fn chat(&self, _messages: &[ChatMsg], tools: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        if let Ok(mut calls) = self.calls.lock() {
            *calls += 1;
        }
        if let Some(report) = &self.wrap_up_report {
                                                                                 
                                                                      
            return Ok(if tools.is_empty() {
                ChatResponse::done(report)
            } else {
                ChatResponse::tool("list_dir", serde_json::json!({ "path": "." }))
            });
        }
        let next = self.script.lock().ok().and_then(|mut s| s.pop_front());
        match next {
            Some(step) => Ok(step),
            None => self.repeat.clone().ok_or(InferError::ScriptExhausted),
        }
    }
}
