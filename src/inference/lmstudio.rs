                                                                             
                                                                              
                                                  

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use ureq::Error as UreqError;
use ureq::http::Uri;
use ureq::unversioned::resolver::{ArrayVec, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::time::Duration as UreqDuration;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport,
};

use crate::endpoint::Endpoint;
use crate::tls::{AgentKind, DeviceIdentity, SpkiPin, pinned_agent, pinned_mtls_agent, select};
use crate::tools::ToolSpec;

use super::{ChatMsg, ChatResponse, InferError, Inference, ToolCallRequest};

                                                                              
                                                     
pub use super::MAX_MODEL_TIMEOUT_SECS;

pub struct LmStudioInference {
    agent: ureq::Agent,
                                                                           
    url: String,
    model: String,
    timeout: Duration,
    max_response_bytes: u64,
}

impl LmStudioInference {
                                                                              
                                        
    pub fn new(
        endpoint: &Endpoint,
        pin: Option<SpkiPin>,
        model: String,
    ) -> Result<LmStudioInference, InferError> {
        Self::build(endpoint, pin, None, model)
    }

                                                                                 
                                                                                  
                                                   
    pub fn new_mtls(
        endpoint: &Endpoint,
        pin: Option<SpkiPin>,
        identity: DeviceIdentity,
        model: String,
    ) -> Result<LmStudioInference, InferError> {
        Self::build(endpoint, pin, Some(identity), model)
    }

                                                                                                 
                                                                                                    
                                                                          
                                                                                               
                                                   
       
                                                                                               
                                                                                                        
                                                                                           
                                                                        
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> LmStudioInference {
        self.timeout = timeout.min(Duration::from_secs(MAX_MODEL_TIMEOUT_SECS));
        self
    }

                                                                                          
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    fn build(
        endpoint: &Endpoint,
        pin: Option<SpkiPin>,
        identity: Option<DeviceIdentity>,
        model: String,
    ) -> Result<LmStudioInference, InferError> {
        let agent = agent_for(endpoint, pin, identity.as_ref())?;
        let base = endpoint.uri().to_string();
        Ok(LmStudioInference {
            agent,
            url: format!("{}/chat/completions", base.trim_end_matches('/')),
            model,
            timeout: Duration::from_secs(300),
            max_response_bytes: 4 * 1024 * 1024,
        })
    }
}

impl Inference for LmStudioInference {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn chat(&self, messages: &[ChatMsg], tools: &[ToolSpec]) -> Result<ChatResponse, InferError> {
        let mut body = json!({
            "model": self.model,
            "messages": messages.iter().map(to_openai).collect::<Vec<_>>(),
            "temperature": 0.2,
            "stream": false,
        });
        if !tools.is_empty()
            && let Some(map) = body.as_object_mut()
        {
            let specs: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            map.insert("tools".to_string(), Value::Array(specs));
            map.insert("tool_choice".to_string(), Value::String("auto".into()));
        }

        let mut response = self
            .agent
            .post(&self.url)
            .config()
            .timeout_global(Some(self.timeout))
            .build()
            .send_json(&body)
            .map_err(|e| InferError::Http(e.to_string()))?;
        let text = response
            .body_mut()
            .with_config()
            .limit(self.max_response_bytes)
            .read_to_string()
            .map_err(|e| InferError::Http(e.to_string()))?;

        parse_completion(&text)
    }
}

                                                                             
                                                                                          
                                                                                  
fn agent_for(
    endpoint: &Endpoint,
    pin: Option<SpkiPin>,
    identity: Option<&DeviceIdentity>,
) -> Result<ureq::Agent, InferError> {
    if let Endpoint::Uds(uds_path) = endpoint {
                                                                                             
                                                                                          
                                                                                                 
                                                                                                   
                                                                                   
        return Ok(uds_agent(uds_path.path()));
    }
    match (
        select(endpoint, identity.is_some(), pin.is_some()),
        identity,
    ) {
        (AgentKind::Plain, _) => Ok(crate::tls::plain_agent()),
        (AgentKind::Mtls, Some(id)) => {
            Ok(pinned_mtls_agent(pin.ok_or(InferError::PinRequired)?, id)?)
        }
                                                                                      
        (_, _) => Ok(pinned_agent(pin.ok_or(InferError::PinRequired)?)?),
    }
}

                                                                                             
                                                                                      
                                                                                
                                                                                                
                                                                                                 
                                                                                                  
                                                                                                 
                  
fn normalize_would_block<T>(r: std::io::Result<T>) -> std::io::Result<T> {
    r.map_err(|e| {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            std::io::Error::new(std::io::ErrorKind::TimedOut, e)
        } else {
            e
        }
    })
}

                                                                                      
                                                                                          
                                                               
                                                                                             
                                                                                           
                                                                                                
                  
struct UdsTransport {
    stream: UnixStream,
    buffers: LazyBuffers,
    timeout_write: Option<UreqDuration>,
    timeout_read: Option<UreqDuration>,
}

impl UdsTransport {
    fn new(stream: UnixStream, buffers: LazyBuffers) -> UdsTransport {
        UdsTransport {
            stream,
            buffers,
            timeout_read: None,
            timeout_write: None,
        }
    }
}

impl std::fmt::Debug for UdsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdsTransport").finish()
    }
}

impl Transport for UdsTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), UreqError> {
        let want = timeout.not_zero();
        if want != self.timeout_write {
            self.stream.set_write_timeout(want.map(|t| *t))?;
            self.timeout_write = want;
        }
        let output = &self.buffers.output()[..amount];
        match normalize_would_block(self.stream.write_all(output)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Err(UreqError::Timeout(timeout.reason))
            }
            Err(e) => Err(e.into()),
        }
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, UreqError> {
        let want = timeout.not_zero();
        if want != self.timeout_read {
            self.stream.set_read_timeout(want.map(|t| *t))?;
            self.timeout_read = want;
        }
        let input = self.buffers.input_append_buf();
        let amount = match normalize_would_block(self.stream.read(input)) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(UreqError::Timeout(timeout.reason));
            }
            Err(e) => return Err(e.into()),
        };
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
                                                                                               
                                                                                                   
                                                                                       
                                                                             
        let mut probe = || -> std::io::Result<bool> {
            self.stream.set_nonblocking(true)?;
            let mut buf = [0u8; 1];
            let r = match self.stream.read(&mut buf) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
                Ok(_) => Ok(false),                                    
                Err(_) => Ok(false),
            };
            self.stream.set_nonblocking(false)?;
            r
        };
        probe().unwrap_or(false)
    }
}

                                                                                               
                                                                                                  
                                                     
#[derive(Debug)]
struct UdsConnector {
    path: std::path::PathBuf,
}

impl Connector for UdsConnector {
    type Out = UdsTransport;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<()>,
    ) -> Result<Option<Self::Out>, UreqError> {
        let _ = chained;
        let stream = UnixStream::connect(&self.path)?;
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(UdsTransport::new(stream, buffers)))
    }
}

                                                                                                 
                                                                                                  
                                                                                                   
                                                                                                   
                                              
#[derive(Debug)]
struct NoopResolver;

impl Resolver for NoopResolver {
    fn resolve(
        &self,
        _uri: &Uri,
        _config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, UreqError> {
                                                                                     
                                                                                        
                                                                
        let mut addrs: ResolvedSocketAddrs = ArrayVec::from_fn(|_| ([127, 0, 0, 1], 1).into());
        addrs.push(([127, 0, 0, 1], 1).into());
        Ok(addrs)
    }
}

                                                                                               
                                                                  
fn uds_agent(path: &Path) -> ureq::Agent {
    ureq::Agent::with_parts(
        crate::tls::base_config(),
        UdsConnector {
            path: path.to_owned(),
        },
        NoopResolver,
    )
}

                                                                         
                                                                         
                                                                              
pub fn list_models(endpoint: &Endpoint, pin: Option<SpkiPin>) -> Result<Vec<String>, InferError> {
    let agent = agent_for(endpoint, pin, None)?;
    let base = endpoint.uri().to_string();
    let url = format!("{}/models", base.trim_end_matches('/'));
    let mut response = agent
        .get(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .call()
        .map_err(|e| InferError::Http(e.to_string()))?;
    let text = response
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .map_err(|e| InferError::Http(e.to_string()))?;
    let v: Value = serde_json::from_str(&text).map_err(|_| InferError::BadResponse)?;
    let data = v["data"].as_array().ok_or(InferError::BadResponse)?;
    Ok(data
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_string))
        .collect())
}

fn to_openai(msg: &ChatMsg) -> Value {
    match msg {
        ChatMsg::System(s) => json!({ "role": "system", "content": s }),
        ChatMsg::User(s) => json!({ "role": "user", "content": s }),
        ChatMsg::Assistant {
            content,
            tool_calls,
        } => {
            let mut v = json!({ "role": "assistant", "content": content });
            if !tool_calls.is_empty()
                && let Some(map) = v.as_object_mut()
            {
                let calls: Vec<Value> = tool_calls
                    .iter()
                    .map(|c| {
                        json!({
                            "id": c.id,
                            "type": "function",
                            "function": { "name": c.name, "arguments": c.arguments }
                        })
                    })
                    .collect();
                map.insert("tool_calls".to_string(), Value::Array(calls));
            }
            v
        }
        ChatMsg::Tool { call_id, content } => json!({
            "role": "tool",
            "tool_call_id": call_id,
            "content": content,
        }),
    }
}

fn parse_completion(text: &str) -> Result<ChatResponse, InferError> {
    let v: Value = serde_json::from_str(text).map_err(|_| InferError::BadResponse)?;
    let message = v["choices"]
        .get(0)
        .map(|c| &c["message"])
        .ok_or(InferError::BadResponse)?;

    let content = message["content"].as_str().map(str::to_string);
    let mut tool_calls = Vec::new();
    if let Some(calls) = message["tool_calls"].as_array() {
        for (i, c) in calls.iter().enumerate() {
            let name = c["function"]["name"]
                .as_str()
                .ok_or(InferError::BadResponse)?;
            let arguments = match &c["function"]["arguments"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),                                  
            };
            let id = c["id"].as_str().map(str::to_string);
            tool_calls.push(ToolCallRequest {
                id: id.unwrap_or_else(|| format!("call-{i}")),
                name: name.to_string(),
                arguments,
            });
        }
    }
    let finish_reason = v["choices"]
        .get(0)
        .and_then(|c| c["finish_reason"].as_str())
        .map(str::to_string);
    Ok(ChatResponse {
        content,
        tool_calls,
        finish_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_mtls_routes_by_endpoint() {
        use rcgen::PublicKeyData;
        use std::os::unix::fs::PermissionsExt;
                                                                                     
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(vec!["client.device".to_string()])
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("d.crt.der");
        let key_path = dir.path().join("d.key.der");
        std::fs::write(&cert_path, cert.der()).unwrap();
        std::fs::write(&key_path, key.serialize_der()).unwrap();
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let pin = SpkiPin::of_spki_der(&key.subject_public_key_info());

        let ext = Endpoint::parse("https://model.example/v1").unwrap();
        let loop_ep = Endpoint::parse("http://127.0.0.1:1234/v1").unwrap();

                                                                                          
                                                                                                
        let id = DeviceIdentity::load(&cert_path, &key_path).unwrap();
        assert!(LmStudioInference::new_mtls(&ext, Some(pin), id, "m".into()).is_ok());
                                                                                                  
        let id2 = DeviceIdentity::load(&cert_path, &key_path).unwrap();
        assert!(LmStudioInference::new_mtls(&loop_ep, Some(pin), id2, "m".into()).is_ok());
                                                                                               
        assert!(matches!(
            LmStudioInference::new(&ext, None, "m".into()),
            Err(InferError::PinRequired)
        ));
    }

    #[test]
    fn parses_tool_call_completion() {
        let raw = r#"{
            "choices": [{ "message": {
                "content": null,
                "tool_calls": [{ "id": "abc", "type": "function",
                    "function": { "name": "read_file", "arguments": "{\"path\":\"src/lib.rs\"}" } }]
            }}]
        }"#;
        let r = parse_completion(raw).unwrap();
        assert!(r.content.is_none());
        assert_eq!(r.tool_calls.len(), 1);
        assert_eq!(r.tool_calls[0].name, "read_file");
    }

    #[test]
    fn parses_final_text_completion() {
        let raw = r#"{ "choices": [{ "message": { "content": "all done" } }] }"#;
        let r = parse_completion(raw).unwrap();
        assert_eq!(r.content.as_deref(), Some("all done"));
        assert!(r.tool_calls.is_empty());
        assert!(!r.length_capped());
    }

    #[test]
    fn length_finish_reason_sets_length_capped() {
        let raw = r#"{ "choices": [{ "finish_reason": "length", "message": { "content": "partial" } }] }"#;
        let r = parse_completion(raw).unwrap();
        assert_eq!(r.content.as_deref(), Some("partial"));
        assert!(r.length_capped());
        let raw2 =
            r#"{ "choices": [{ "finish_reason": "stop", "message": { "content": "done" } }] }"#;
        assert!(!parse_completion(raw2).unwrap().length_capped());
    }

    #[test]
    fn finish_reason_is_carried_verbatim_and_absent_when_missing() {
        let raw = r#"{ "choices": [{ "finish_reason": "context_length", "message": { "content": "x" } }] }"#;
        let r = parse_completion(raw).unwrap();
        assert_eq!(r.finish_reason.as_deref(), Some("context_length"));
        assert!(!r.length_capped());
        let raw = r#"{ "choices": [{ "message": { "content": "x" } }] }"#;
        let r = parse_completion(raw).unwrap();
        assert_eq!(r.finish_reason, None);
    }

    #[test]
    fn a_non_string_finish_reason_is_none_and_an_empty_string_is_carried() {
        for v in [
            json!(null),
            json!(3),
            json!({ "a": 1 }),
            json!(["length"]),
            json!(true),
        ] {
            let raw =
                json!({ "choices": [{ "finish_reason": v.clone(), "message": { "content": "x" } }] })
                    .to_string();
            let r = parse_completion(&raw).unwrap();
            assert_eq!(
                r.content.as_deref(),
                Some("x"),
                "reached the parsed completion for {v}"
            );
            assert_eq!(r.finish_reason, None, "carried for {v}");
            assert!(!r.length_capped(), "predicate at {v}");
        }

        let raw = json!({ "choices": [{ "finish_reason": "", "message": { "content": "x" } }] })
            .to_string();
        let r = parse_completion(&raw).unwrap();
        assert_eq!(
            r.content.as_deref(),
            Some("x"),
            "reached the parsed completion for the empty string"
        );
        assert_eq!(
            r.finish_reason.as_deref(),
            Some(""),
            "carried for the empty string"
        );
        assert!(!r.length_capped(), "predicate at the empty string");
    }

                                                                                              
                                                                                            
    const BACKEND_FINISH_REASONS: [&str; 9] = [
        "stop",
        "tool_calls",
        "length",
        "context_length",
        "wall_clock",
        "capacity",
        "body_bytes",
        "memory_pressure",
        "  Stop_Reason ü  ",
    ];

    #[test]
    fn chat_carries_each_backend_finish_reason_verbatim_over_a_loopback_server() {
        let bodies: Vec<String> = BACKEND_FINISH_REASONS
            .iter()
            .map(|r| {
                json!({ "choices": [{ "finish_reason": r, "message": { "content": "x" } }] })
                    .to_string()
            })
            .collect();
        let (port, server) = replaying_http(bodies);
        let ep = Endpoint::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap();
        let inf = LmStudioInference::new(&ep, None, "m".into()).unwrap();
        for r in BACKEND_FINISH_REASONS {
            let resp = inf.chat(&[ChatMsg::User("hi".into())], &[]).unwrap();
            assert_eq!(
                resp.content.as_deref(),
                Some("x"),
                "reached the parsed completion for {r:?}"
            );
            assert_eq!(resp.finish_reason.as_deref(), Some(r), "carried for {r:?}");
            assert_eq!(resp.length_capped(), r == "length", "predicate at {r:?}");
        }
        server.join().unwrap();
    }

                                                                                       
                                                                                     
    fn replaying_http(bodies: Vec<String>) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for body in bodies {
                let (mut sock, _) = listener.accept().unwrap();
                let mut buf: Vec<u8> = Vec::new();
                let head_end = loop {
                    let mut chunk = [0u8; 1024];
                    let n = sock.read(&mut chunk).unwrap();
                    assert!(n > 0, "closed before the request head ended");
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let head = std::str::from_utf8(&buf[..head_end]).unwrap().to_string();
                let len: usize = head
                    .lines()
                    .find_map(|l| {
                        let low = l.to_ascii_lowercase();
                        low.strip_prefix("content-length:")
                            .map(|v| v.trim().to_string())
                    })
                    .expect("the request carried no content-length")
                    .parse()
                    .unwrap();
                while buf.len() < head_end + len {
                    let mut chunk = [0u8; 1024];
                    let n = sock.read(&mut chunk).unwrap();
                    assert!(n > 0, "closed mid-body");
                    buf.extend_from_slice(&chunk[..n]);
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                sock.write_all(resp.as_bytes()).unwrap();
            }
        });
        (port, handle)
    }

                                                          
    fn canned_http(body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(resp.as_bytes()).unwrap();
        });
        (port, handle)
    }

    #[test]
    fn default_timeout_is_the_shipped_300s() {
        let ep = Endpoint::parse("http://127.0.0.1:9/v1").unwrap();
        let inf = LmStudioInference::new(&ep, None, "m".into()).unwrap();
        assert_eq!(inf.timeout(), Duration::from_secs(300));
    }

    #[test]
    fn with_timeout_clamps_the_panic_class_values() {
                                                                            
                                                                              
                                                                                       
                                                             
        let ep = Endpoint::parse("http://127.0.0.1:9/v1").unwrap();
        let inf = LmStudioInference::new(&ep, None, "m".into())
            .unwrap()
            .with_timeout(Duration::from_secs(u64::MAX));
        assert_eq!(inf.timeout(), Duration::from_secs(MAX_MODEL_TIMEOUT_SECS));
                                                  
        let inf = LmStudioInference::new(&ep, None, "m".into())
            .unwrap()
            .with_timeout(Duration::from_secs(42));
        assert_eq!(inf.timeout(), Duration::from_secs(42));
    }

    #[test]
    fn with_timeout_bounds_a_stalled_server_with_a_typed_error() {
        use std::io::Read;
                                                                                                   
                                                                                                     
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf);
            std::thread::sleep(Duration::from_secs(8));
        });
        let ep = Endpoint::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap();
        let inf = LmStudioInference::new(&ep, None, "m".into())
            .unwrap()
            .with_timeout(Duration::from_secs(1));
        assert_eq!(inf.timeout(), Duration::from_secs(1));
        let start = std::time::Instant::now();
        let err = inf
            .chat(&[ChatMsg::User("hi".into())], &[])
            .expect_err("a stalled server must not read as success");
        assert!(
            matches!(err, InferError::Http(_)),
            "typed Http, got {err:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(6),
            "the override bound the call ({}s elapsed)",
            start.elapsed().as_secs()
        );
                                                                                                   
                                      
    }

    #[test]
    fn lists_models_from_openai_endpoint() {
        let (port, server) = canned_http(
            r#"{"object":"list","data":[{"id":"qwen/qwen3-coder-next"},{"id":"embed-v1"}]}"#,
        );
        let ep = Endpoint::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap();
        let models = list_models(&ep, None).unwrap();
        assert_eq!(models, ["qwen/qwen3-coder-next", "embed-v1"]);
        server.join().unwrap();
    }

    #[test]
    fn list_models_reports_unreachable_server() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let ep = Endpoint::parse(&format!("http://127.0.0.1:{port}/v1")).unwrap();
        assert!(matches!(list_models(&ep, None), Err(InferError::Http(_))));
    }

    #[test]
    fn agent_for_uds_never_needs_pin_or_identity() {
                                                                                              
                                                                                                 
        let ep = Endpoint::parse("unix:/run/creatine/creatine.sock").unwrap();
        assert!(agent_for(&ep, None, None).is_ok());
    }

                                                                                        
    fn canned_uds(
        body: &'static str,
    ) -> (
        std::path::PathBuf,
        std::thread::JoinHandle<()>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creatine.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
                                                                                            
                                                                                               
                                                                                        
            sock.set_read_timeout(Some(std::time::Duration::from_millis(500)))
                .unwrap();
            let mut buf = [0u8; 4096];
            loop {
                match sock.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => continue,
                    Err(_) => break,                                            
                }
            }
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(resp.as_bytes()).unwrap();
        });
        (path, handle, dir)
    }

    #[test]
    fn uds_round_trip_chat_completion() {
        let raw = r#"{ "choices": [{ "message": { "content": "hi from uds" } }] }"#;
        let (path, server, _dir) = canned_uds(raw);
        let ep = Endpoint::parse(&format!("unix:{}", path.display())).unwrap();
        let infer = LmStudioInference::new(&ep, None, "m".into()).unwrap();
        let resp = infer.chat(&[ChatMsg::User("hello".into())], &[]).unwrap();
        assert_eq!(resp.content.as_deref(), Some("hi from uds"));
        server.join().unwrap();
    }

    #[test]
    fn uds_list_models_round_trip() {
        let (path, server, _dir) =
            canned_uds(r#"{"object":"list","data":[{"id":"qwen/qwen3-coder-next"}]}"#);
        let ep = Endpoint::parse(&format!("unix:{}", path.display())).unwrap();
        let models = list_models(&ep, None).unwrap();
        assert_eq!(models, ["qwen/qwen3-coder-next"]);
        server.join().unwrap();
    }

    #[test]
    fn uds_nonexistent_path_fails_socket_level_not_dns() {
                                                                                         
                                                                                      
                                                                  
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.sock");
        let ep = Endpoint::parse(&format!("unix:{}", path.display())).unwrap();
        let err = match list_models(&ep, None) {
            Err(InferError::Http(msg)) => msg,
            other => panic!("expected InferError::Http, got {other:?}"),
        };
        let lower = err.to_lowercase();
        assert!(
            !lower.contains("dns") && !lower.contains("resolve") && !lower.contains("lookup"),
            "expected a socket-level error, got: {err}"
        );
    }

    #[test]
    fn uds_stalled_peer_surfaces_a_clean_timeout_not_a_raw_os_error() {
                                                                                              
                                                                                                     
                                                                                                   
                                                                                                   
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stall.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (_sock, _) = listener.accept().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));                                       
        });
        let agent = uds_agent(&path);
        let err = agent
            .get("http://localhost/v1/models")
            .config()
            .timeout_global(Some(std::time::Duration::from_millis(150)))
            .build()
            .call()
            .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("timeout"),
            "expected a timeout error, got: {msg}"
        );
        assert!(!msg.contains("os error"), "leaked a raw OS error: {msg}");
    }
}
