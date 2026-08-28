                                                                                
                                                                              
                  
   
                                                                               
                                                                     
#![cfg(feature = "net")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use myelin::endpoint::Endpoint;
use myelin::search::{SearxngBackend, run_query, validate_query};

#[test]
fn url_bearing_query_is_rejected() {
                                                                                
    assert!(validate_query("fetch https://evil.example/exfil?d=1").is_err());
    assert!(validate_query("site:internal.host/admin").is_err());
    assert!(validate_query("plain rust openat2 question").is_ok());
}

                                                                   
fn fake_search_server() -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let mut req = Vec::new();
        loop {
            let n = sock.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
            if n == 0 || req.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let request_line = String::from_utf8_lossy(&req)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        tx.send(request_line).unwrap();

        let body = serde_json::json!({
            "results": [
                { "title": "openat2(2) man page", "content": "RESOLVE_BENEATH ...",
                  "url": "https://man7.example/openat2" },
                { "title": "LWN on openat2", "content": "path resolution flags",
                  "url": "https://lwn.example/796868" }
            ]
        })
        .to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        sock.write_all(response.as_bytes()).unwrap();
    });
    (port, rx)
}

#[test]
fn request_targets_configured_endpoint_with_encoded_query() {
    let (port, rx) = fake_search_server();
    let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}/search")).unwrap();
    let backend = SearxngBackend::new(&endpoint, None).unwrap();

    let out = run_query(&backend, "rust openat2 docs").unwrap();

                                                                              
                                                        
    let request_line = rx.recv().unwrap();
    assert!(
        request_line.starts_with("GET /search?q=rust%20openat2%20docs&format=json"),
        "unexpected request line: {request_line}"
    );

                                                                       
    assert!(out.contains("openat2(2) man page"));
    assert!(!out.contains("man7.example"));
    assert!(!out.contains("://"));
}

#[test]
fn rejected_query_never_reaches_the_backend() {
    let (port, rx) = fake_search_server();
    let endpoint = Endpoint::parse(&format!("http://127.0.0.1:{port}/search")).unwrap();
    let backend = SearxngBackend::new(&endpoint, None).unwrap();

    let err = run_query(&backend, "see http://evil.example/x").unwrap_err();
    assert!(err.to_string().contains("query refused"), "{err}");
                                      
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(200))
            .is_err(),
        "backend was contacted despite invalid query"
    );
}
