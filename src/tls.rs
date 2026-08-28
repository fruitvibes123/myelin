                                            
   
                                                                     
                                                                              
                                                                              
                                                                           
                                                                             
                                    
   
                                                                              
                                                                           
                                                                               
                                                                             
                                                                              
                                                             

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned};
use sha2::{Digest, Sha256};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, TcpConnector,
    Transport, TransportAdapter,
};

use crate::endpoint::Endpoint;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("pin must be 64 hex chars (sha256 of the SPKI)")]
    BadPin,
    #[error("certificate could not be parsed")]
    BadCert,
    #[error("tls setup failed: {0}")]
    Rustls(#[from] rustls::Error),
                                                                      
    #[error("device key {0} must be owner-only (0600)")]
    KeyNotOwnerOnly(PathBuf),
                                                                                                  
    #[error("cannot read device identity {path}: {source}")]
    IdentityIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
                                                                                                
    #[error("device private key is invalid")]
    BadKey,
                                             
    #[error("device certificate is invalid")]
    BadIdentityCert,
}

                                                                        
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpkiPin([u8; 32]);

impl SpkiPin {
                                                      
    pub fn from_hex(s: &str) -> Result<SpkiPin, TlsError> {
        fn hexval(b: u8) -> Option<u8> {
            match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            }
        }
        let bytes = s.trim().as_bytes();
        if bytes.len() != 64 {
            return Err(TlsError::BadPin);
        }
        let mut out = [0u8; 32];
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            let (hi, lo) = (hexval(pair[0]), hexval(pair[1]));
            match (hi, lo) {
                (Some(h), Some(l)) => out[i] = (h << 4) | l,
                _ => return Err(TlsError::BadPin),
            }
        }
        Ok(SpkiPin(out))
    }

                                                                            
                                                                    
    pub fn of_spki_der(spki: &[u8]) -> SpkiPin {
        SpkiPin(Sha256::digest(spki).into())
    }

                                                      
    pub fn of_cert_der(cert: &[u8]) -> Result<SpkiPin, TlsError> {
        let (_, parsed) =
            x509_parser::parse_x509_certificate(cert).map_err(|_| TlsError::BadCert)?;
        Ok(SpkiPin::of_spki_der(parsed.tbs_certificate.subject_pki.raw))
    }
}

                                                                          
                                                                           
                                                                                 
                                                                                 
                             
pub struct DeviceIdentity {
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
}

impl DeviceIdentity {
                                                                                 
                                                                                  
                                                                               
                                                                             
                                                                                  
    pub fn load(cert_der_path: &Path, key_der_path: &Path) -> Result<DeviceIdentity, TlsError> {
                                              
        let cert_bytes = std::fs::read(cert_der_path).map_err(|source| TlsError::IdentityIo {
            path: cert_der_path.to_path_buf(),
            source,
        })?;
        SpkiPin::of_cert_der(&cert_bytes).map_err(|_| TlsError::BadIdentityCert)?;
        let certs = vec![CertificateDer::from(cert_bytes)];

                                                                                  
                                                                                          
        let meta = std::fs::metadata(key_der_path).map_err(|source| TlsError::IdentityIo {
            path: key_der_path.to_path_buf(),
            source,
        })?;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(TlsError::KeyNotOwnerOnly(key_der_path.to_path_buf()));
        }
        let key_bytes = std::fs::read(key_der_path).map_err(|source| TlsError::IdentityIo {
            path: key_der_path.to_path_buf(),
            source,
        })?;
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_bytes));

        Ok(DeviceIdentity { certs, key })
    }
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                                                                                  
        f.debug_struct("DeviceIdentity")
            .field("certs", &self.certs.len())
            .field("key", &"<redacted>")
            .finish()
    }
}

                                                                 
#[derive(Debug)]
pub struct PinnedKeyVerifier {
    pin: SpkiPin,
    provider: Arc<CryptoProvider>,
}

impl PinnedKeyVerifier {
    pub fn new(pin: SpkiPin, provider: Arc<CryptoProvider>) -> PinnedKeyVerifier {
        PinnedKeyVerifier { pin, provider }
    }
}

impl ServerCertVerifier for PinnedKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = SpkiPin::of_cert_der(end_entity.as_ref()).map_err(|_| {
            rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding)
        })?;
        if presented == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
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

                                                            
   
                                                                                 
                                                                                 
                                                                           
                                      
pub fn pinned_client_config(pin: SpkiPin) -> Result<rustls::ClientConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedKeyVerifier::new(pin, provider)))
        .with_no_client_auth();
    Ok(config)
}

                                                                               
                                                                          
                                                                                 
                                                                              
                                                                                
                                                                                   
                                                                               
                                                                                    
                                                
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AgentKind {
    Plain,
    ServerPin,
    Mtls,
}

pub(crate) fn select(endpoint: &Endpoint, has_identity: bool, has_pin: bool) -> AgentKind {
    match endpoint {
        Endpoint::Loopback(_) if has_pin && endpoint.uri().scheme_str() == Some("https") => {
            AgentKind::ServerPin
        }
        Endpoint::Loopback(_) => AgentKind::Plain,
                                                                               
        Endpoint::Uds(_) => AgentKind::Plain,
        Endpoint::External(_) if has_identity => AgentKind::Mtls,
        Endpoint::External(_) => AgentKind::ServerPin,
    }
}

                                                                                  
                                                                                
                                                                               
                                                                                  
                                                                                      
                                                                                  
                                                                           
pub(crate) fn base_config() -> ureq::config::Config {
    ureq::config::Config::builder()
        .proxy(None)
        .max_redirects(0)
        .build()
}

                                                                                   
                                                              
pub(crate) fn plain_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(base_config())
}

                                                                            
                                                                            
                                                        
pub fn pinned_agent(pin: SpkiPin) -> Result<ureq::Agent, TlsError> {
    let config = Arc::new(pinned_client_config(pin)?);
    let connector = TcpConnector::default().chain(PinnedTlsConnector { config });
    Ok(ureq::Agent::with_parts(
        base_config(),
        connector,
        DefaultResolver::default(),
    ))
}

                                                                              
                                                                               
                                                                           
                                                                                 
                                                                                
                                                
                                                                             
                                                                                 
                                                                              
                                                                   
pub fn pinned_mtls_client_config(
    pin: SpkiPin,
    identity: &DeviceIdentity,
) -> Result<rustls::ClientConfig, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedKeyVerifier::new(pin, provider)))
                                                                                   
                                                                                   
                                                                                           
        .with_client_auth_cert(identity.certs.clone(), identity.key.clone_key())
        .map_err(|_| TlsError::BadKey)?;
    Ok(config)
}

                                                                            
                                                                               
pub fn pinned_mtls_agent(pin: SpkiPin, identity: &DeviceIdentity) -> Result<ureq::Agent, TlsError> {
    let config = Arc::new(pinned_mtls_client_config(pin, identity)?);
    let connector = TcpConnector::default().chain(PinnedTlsConnector { config });
    Ok(ureq::Agent::with_parts(
        base_config(),
        connector,
        DefaultResolver::default(),
    ))
}

                                                                            
                                                                          
                                                                     
pub struct PinnedTlsConnector {
    config: Arc<rustls::ClientConfig>,
}

impl std::fmt::Debug for PinnedTlsConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedTlsConnector").finish_non_exhaustive()
    }
}

impl<In: Transport> Connector<In> for PinnedTlsConnector {
    type Out = Either<In, PinnedTlsTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Err(ureq::Error::Tls("pinned TLS requires a chained transport"));
        };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }

        let host = details
            .uri
            .host()
            .ok_or(ureq::Error::Tls("pinned TLS needs a host"))?;
        let name: ServerName<'_> = host
            .try_into()
            .map_err(|_| ureq::Error::Tls("invalid server name for pinned TLS"))?;
        let conn = ClientConnection::new(self.config.clone(), name.to_owned())?;
        let stream = StreamOwned {
            conn,
            sock: TransportAdapter::new(transport.boxed()),
        };
        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );
        Ok(Some(Either::B(PinnedTlsTransport { buffers, stream })))
    }
}

pub struct PinnedTlsTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}

impl std::fmt::Debug for PinnedTlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedTlsTransport").finish_non_exhaustive()
    }
}

impl Transport for PinnedTlsTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        use std::io::Write;
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        use std::io::Read;
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_is_key_only_on_external() {
        let loop_ep = Endpoint::parse("http://127.0.0.1:1234/v1").unwrap();
        let loop_https = Endpoint::parse("https://localhost:1234/v1").unwrap();
        let ext_ep = Endpoint::parse("https://model.example/v1").unwrap();
                                                                                    
                                                                                  
        assert_eq!(select(&loop_ep, true, false), AgentKind::Plain);
        assert_eq!(select(&loop_ep, false, false), AgentKind::Plain);
                                                             
        assert_eq!(select(&loop_ep, true, true), AgentKind::Plain);
                                                                                
        assert_eq!(select(&loop_https, true, true), AgentKind::ServerPin);
        assert_eq!(select(&loop_https, false, true), AgentKind::ServerPin);
                                                                         
        assert_eq!(select(&loop_https, false, false), AgentKind::Plain);
                                                                                   
        assert_eq!(select(&ext_ep, true, true), AgentKind::Mtls);
        assert_eq!(select(&ext_ep, false, true), AgentKind::ServerPin);
    }

    #[test]
    fn select_treats_uds_as_plain_same_tier_as_loopback() {
        let ep = Endpoint::parse("unix:/run/creatine/creatine.sock").unwrap();
        assert_eq!(select(&ep, true, false), AgentKind::Plain);
        assert_eq!(select(&ep, false, false), AgentKind::Plain);
    }
}
