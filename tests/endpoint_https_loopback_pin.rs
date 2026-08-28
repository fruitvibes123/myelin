                                                                                  
                                                                                    
#![cfg(feature = "net")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

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

                                                                                
fn server(fx: &Fx) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let cert = fx.cert.clone();
    let key = fx.key_pkcs8.clone();
    let h = std::thread::spawn(move || {
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
        let mut buf = [0u8; 4096];
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
        let body = r#"{"choices":[{"message":{"content":"served over tls"}}]}"#;
        let _ = tls.write_all(
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .as_bytes(),
        );
        let _ = tls.flush();
                                                                             
                                                                                 
        loop {
            match tls.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
    (port, h)
}

#[test]
fn https_loopback_matching_pin_handshakes() {
    let fx = fixture("localhost");
    let (port, srv) = server(&fx);
    let ep = Endpoint::parse(&format!("https://localhost:{port}/v1")).unwrap();
    assert!(ep.is_loopback());
    let inf = LmStudioInference::new(&ep, Some(fx.pin), "m".into()).unwrap();
    let r = inf.chat(&[ChatMsg::User("hi".into())], &[]);
    let _ = srv.join();
    assert!(
        r.is_ok(),
        "the correct pin did not carry the https-loopback handshake: {r:?}"
    );
}

#[test]
fn https_loopback_wrong_pin_refuses() {
    let fx = fixture("localhost");
    let wrong = fixture("localhost").pin;                             
    let (port, srv) = server(&fx);
    let ep = Endpoint::parse(&format!("https://localhost:{port}/v1")).unwrap();
    let inf = LmStudioInference::new(&ep, Some(wrong), "m".into()).unwrap();
    let r = inf.chat(&[ChatMsg::User("hi".into())], &[]);
    let _ = srv.join();
    assert!(
        r.is_err(),
        "a wrong pin must refuse the https-loopback handshake, got {r:?}"
    );
}
