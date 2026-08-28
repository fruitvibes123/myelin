                                                                                   
                                                                                 
                                                                                
                                                                                   
                                                                               
#![cfg(feature = "net")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use myelin::endpoint::Endpoint;
use myelin::inference::lmstudio::list_models;

fn recorder(body: &'static str) -> (u16, mpsc::Receiver<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            sock.set_read_timeout(Some(Duration::from_millis(800)))
                .unwrap();
            let mut buf = [0u8; 8192];
            let mut got = Vec::new();
            while let Ok(n) = sock.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                if got.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = sock.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            );
            let _ = tx.send(());
        }
    });
    (port, rx)
}

#[test]
fn ambient_http_proxy_does_not_divert_a_loopback_endpoint() {
    let (proxy_port, proxy_rx) = recorder(r#"{"data":[{"id":"VIA-PROXY"}]}"#);
    let (model_port, model_rx) = recorder(r#"{"data":[{"id":"DIRECT"}]}"#);

                                                                               
                                                                         
    unsafe {
        std::env::set_var("HTTP_PROXY", format!("http://127.0.0.1:{proxy_port}"));
    }
    let ep = Endpoint::parse(&format!("http://127.0.0.1:{model_port}/v1")).unwrap();
    let models = list_models(&ep, None);
    unsafe {
        std::env::remove_var("HTTP_PROXY");
    }

    assert_eq!(
        models.unwrap(),
        ["DIRECT"],
        "the model server, not the proxy, must answer",
    );
    assert!(
        model_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
        "the model server received no request",
    );
    assert!(
        proxy_rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "the proxy received the request — HTTP_PROXY diverted a loopback endpoint",
    );
}
