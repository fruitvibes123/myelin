                                                                                    
                                                                                     
                                                                               
                           
#![cfg(feature = "net")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use rcgen::PublicKeyData;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use myelin::endpoint::Endpoint;
use myelin::inference::{ChatMsg, Inference, LmStudioInference};
use myelin::tls::SpkiPin;

struct Fx {
    cert: CertificateDer<'static>,
    key_pkcs8: Vec<u8>,
    pin: SpkiPin,
}

fn fixture(san: &str) -> Fx {
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec![san.to_string()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    Fx {
        cert: cert.der().clone(),
        key_pkcs8: key.serialize_der(),
        pin: SpkiPin::of_spki_der(&key.subject_public_key_info()),
    }
}

                                                                  
fn plain_recorder() -> (u16, mpsc::Receiver<()>) {
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
            let body = r#"{"choices":[{"message":{"content":"PLAINTEXT PEER ANSWERED"}}]}"#;
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

                                                                                      
fn redirecting_tls(fx: &Fx, location: String) -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let cert = fx.cert.clone();
    let key = fx.key_pkcs8.clone();
    std::thread::spawn(move || {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
            )
            .unwrap();
        let Ok((mut tcp, _)) = listener.accept() else {
            return;
        };
        let mut conn = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        let mut tls = rustls::Stream::new(&mut conn, &mut tcp);
        let mut buf = [0u8; 8192];
        let mut req = Vec::new();
        loop {
            match tls.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => {
                    req.extend_from_slice(&buf[..n]);
                    if req.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(_) => return,
            }
        }
        let _ = tls.write_all(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            )
            .as_bytes(),
        );
        let _ = tls.flush();
        let mut sink = [0u8; 1024];
        loop {
            match tls.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    port
}

#[test]
fn pinned_agent_does_not_follow_302_to_plain_http() {
    let fx = fixture("localhost");
    let (plain_port, plain_rx) = plain_recorder();
    let tls_port = redirecting_tls(
        &fx,
        format!("http://127.0.0.1:{plain_port}/v1/chat/completions"),
    );
    let ep = Endpoint::parse(&format!("https://localhost:{tls_port}/v1")).unwrap();
    let inf = LmStudioInference::new(&ep, Some(fx.pin), "m".into()).unwrap();
    let _ = inf.chat(&[ChatMsg::User("CANARY-PROMPT-DO-NOT-LEAK".into())], &[]);
    assert!(
        plain_rx.recv_timeout(Duration::from_secs(2)).is_err(),
        "the client followed the 302 onto plain http — the plaintext peer received a request",
    );
}
