                                                                           
                                                                           
                             
   
                                                                                  
                                  
#![cfg(feature = "net")]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread::JoinHandle;

use rcgen::PublicKeyData;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

use myelin::endpoint::{Endpoint, EndpointError};
use myelin::tls::{PinnedKeyVerifier, SpkiPin, pinned_agent};

struct Fixture {
    cert: CertificateDer<'static>,
    key_pkcs8: Vec<u8>,
                                                                     
                                                                            
    pin: SpkiPin,
}

fn fixture(san: &str) -> Fixture {
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec![san.to_string()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    Fixture {
        cert: cert.der().clone(),
        key_pkcs8: key.serialize_der(),
        pin: SpkiPin::of_spki_der(&key.subject_public_key_info()),
    }
}

fn wrong_pin() -> SpkiPin {
    SpkiPin::of_spki_der(b"not the right key at all")
}

#[test]
fn external_rejects_non_https() {
    assert_eq!(
        Endpoint::parse("http://model.example/v1"),
        Err(EndpointError::ExternalNotHttps)
    );
    let uri: ureq::http::Uri = "http://model.example/v1".parse().unwrap();
    assert_eq!(
        Endpoint::external(uri),
        Err(EndpointError::ExternalNotHttps)
    );
    assert!(matches!(
        Endpoint::parse("https://model.example/v1"),
        Ok(Endpoint::External(_))
    ));
}

#[test]
fn loopback_may_use_plain_http() {
    assert!(
        Endpoint::parse("http://127.0.0.1:1234/v1")
            .unwrap()
            .is_loopback()
    );
    assert!(
        Endpoint::parse("http://localhost:8080/q")
            .unwrap()
            .is_loopback()
    );
}

#[test]
fn pin_parses_from_hex_and_rejects_garbage() {
    let hex = "ab".repeat(32);
    assert!(SpkiPin::from_hex(&hex).is_ok());
    assert!(SpkiPin::from_hex("ab").is_err());
    assert!(SpkiPin::from_hex(&"zz".repeat(32)).is_err());
}

#[test]
fn cert_extraction_matches_rcgen_spki() {
    let fx = fixture("model.example");
    assert_eq!(SpkiPin::of_cert_der(fx.cert.as_ref()).unwrap(), fx.pin);
}

#[test]
fn verifier_accepts_matching_pin_and_rejects_wrong_pin() {
    let fx = fixture("model.example");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let name = ServerName::try_from("model.example").unwrap();

    let ok = PinnedKeyVerifier::new(fx.pin, provider.clone());
    assert!(
        ok.verify_server_cert(&fx.cert, &[], &name, &[], UnixTime::now())
            .is_ok()
    );

    let bad = PinnedKeyVerifier::new(wrong_pin(), provider);
    assert!(
        bad.verify_server_cert(&fx.cert, &[], &name, &[], UnixTime::now())
            .is_err()
    );
}

                                                                          
                                                     
fn one_shot_server(fx: &Fixture) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let cert = fx.cert.clone();
    let key = fx.key_pkcs8.clone();
    let handle = std::thread::spawn(move || {
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
        let (mut tcp, _) = listener.accept().unwrap();
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
        let _ = tls.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 6\r\n\r\npinned");
    });
    (port, handle)
}

#[test]
fn handshake_succeeds_with_matching_pin() {
    let fx = fixture("localhost");
    let (port, server) = one_shot_server(&fx);

    let agent = pinned_agent(fx.pin).unwrap();
    let mut res = agent
        .get(format!("https://localhost:{port}/check"))
        .call()
        .expect("pinned handshake should succeed");
    assert_eq!(res.status(), 200);
    assert_eq!(res.body_mut().read_to_string().unwrap(), "pinned");
    server.join().unwrap();
}

#[test]
fn handshake_refused_with_wrong_pin() {
    let fx = fixture("localhost");
    let (port, server) = one_shot_server(&fx);

    let agent = pinned_agent(wrong_pin()).unwrap();
    let err = agent
        .get(format!("https://localhost:{port}/check"))
        .call()
        .expect_err("wrong pin must refuse the connection");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("InvalidCertificate") || msg.contains("certificate"),
        "unexpected error: {msg}"
    );
    server.join().unwrap();
}

                                                      

use rustls::DistinguishedName;
use rustls::client::danger::HandshakeSignatureValid;
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, SignatureScheme};

use myelin::tls::{DeviceIdentity, TlsError, pinned_mtls_agent};

                                                                              
                                                                               
                                                                                 
                                     
#[derive(Debug)]
struct RequireAnyClientCert {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ClientCertVerifier for RequireAnyClientCert {
    fn client_auth_mandatory(&self) -> bool {
        true
    }
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

                                                                  
                                                                                  
                                       
fn mtls_server(server_fx: &Fixture) -> (u16, JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let cert = server_fx.cert.clone();
    let key = server_fx.key_pkcs8.clone();
    let handle = std::thread::spawn(move || {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = Arc::new(RequireAnyClientCert {
            provider: provider.clone(),
        });
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(verifier)
            .with_single_cert(
                vec![cert],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
            )
            .unwrap();
        let Ok((mut tcp, _)) = listener.accept() else {
            return;
        };
        let mut conn = match rustls::ServerConnection::new(Arc::new(config)) {
            Ok(c) => c,
            Err(_) => return,
        };
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
        let _ = tls.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\nmtls");
    });
    (port, handle)
}

                                                                                   
                                                                 
fn device_identity(client_fx: &Fixture) -> (tempfile::TempDir, DeviceIdentity) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("device.crt.der");
    let key_path = dir.path().join("device.key.der");
    std::fs::write(&cert_path, client_fx.cert.as_ref()).unwrap();
    std::fs::write(&key_path, &client_fx.key_pkcs8).unwrap();
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let id = DeviceIdentity::load(&cert_path, &key_path).unwrap();
    (dir, id)
}

#[test]
fn mtls_no_cert_fails_and_with_cert_succeeds() {
    let client_fx = fixture("client.device");
    let (_dir, identity) = device_identity(&client_fx);

                                                                                  
                                                                                
                                                                      
    let server_fx = fixture("localhost");
    let (port, server) = mtls_server(&server_fx);
    let err = pinned_agent(server_fx.pin)
        .unwrap()
        .get(format!("https://localhost:{port}/m"))
        .call()
        .expect_err("a client-auth-mandatory server must refuse a no-cert client");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("Tls")
            || msg.contains("certificate")
            || msg.contains("Io")
            || msg.contains("handshake"),
        "expected a TLS/handshake failure, got: {msg}"
    );
    let _ = server.join();

                                                                            
    let server_fx2 = fixture("localhost");
    let (port2, server2) = mtls_server(&server_fx2);
    let mut res = pinned_mtls_agent(server_fx2.pin, &identity)
        .unwrap()
        .get(format!("https://localhost:{port2}/m"))
        .call()
        .expect("the client identity should authenticate to a client-auth server");
    assert_eq!(res.status(), 200);
    assert_eq!(res.body_mut().read_to_string().unwrap(), "mtls");
    server2.join().unwrap();
}

#[test]
fn mtls_still_pins_the_server() {
                                                                               
                                                               
    let client_fx = fixture("client.device");
    let (_dir, identity) = device_identity(&client_fx);
    let server_fx = fixture("localhost");
    let (port, server) = mtls_server(&server_fx);
    let err = pinned_mtls_agent(wrong_pin(), &identity)
        .unwrap()
        .get(format!("https://localhost:{port}/m"))
        .call()
        .expect_err("a foreign server must be refused even under mTLS");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("InvalidCertificate") || msg.contains("certificate"),
        "unexpected error: {msg}"
    );
    let _ = server.join();
}

#[test]
fn device_identity_requires_0600_key() {
    use std::os::unix::fs::PermissionsExt;
    let fx = fixture("client.device");
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("d.crt.der");
    let key_path = dir.path().join("d.key.der");
    std::fs::write(&cert_path, fx.cert.as_ref()).unwrap();
    std::fs::write(&key_path, &fx.key_pkcs8).unwrap();
                                          
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        DeviceIdentity::load(&cert_path, &key_path),
        Err(TlsError::KeyNotOwnerOnly(_))
    ));
                     
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(DeviceIdentity::load(&cert_path, &key_path).is_ok());
}

#[test]
fn device_identity_rejects_garbage_cert() {
    use std::os::unix::fs::PermissionsExt;
    let fx = fixture("client.device");
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("bad.crt.der");
    let key_path = dir.path().join("d.key.der");
    std::fs::write(&cert_path, b"not a der certificate").unwrap();
    std::fs::write(&key_path, &fx.key_pkcs8).unwrap();
    std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        DeviceIdentity::load(&cert_path, &key_path),
        Err(TlsError::BadIdentityCert)
    ));
}

#[test]
fn device_identity_debug_redacts_key_and_cert() {
    let client_fx = fixture("client.device");
    let (_dir, identity) = device_identity(&client_fx);
                                                                                             
    assert_eq!(
        format!("{identity:?}"),
        r#"DeviceIdentity { certs: 1, key: "<redacted>" }"#
    );
}
