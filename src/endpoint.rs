                                                                                  
                                                                                  
                                                                                   

use std::net::IpAddr;

use ureq::http::Uri;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EndpointError {
    #[error("invalid endpoint url")]
    Invalid,
    #[error("endpoint url must have a scheme and a host")]
    Incomplete,
    #[error("endpoint scheme must be http or https")]
    BadScheme,
    #[error("external endpoints must use https")]
    ExternalNotHttps,
    #[error("external endpoints must not be a literal loopback host")]
    ExternalIsLoopback,
    #[error("a unix: endpoint path must be absolute")]
    UdsPathNotAbsolute,
}

                                                                                
                                                                              
                                                                                
#[derive(Debug, Clone, PartialEq)]
pub struct LoopbackUri(Uri);

                                                                           
                                                                                    
                                                                             
                                                                                 
                                            
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalUri(Uri);

                                                                                                    
                                                                                                     
                                                                                                   
                                                                                               
                                                                                                    
                                                                                               
                                                                                                     
                                                                                                    
                                                                                                    
                              
#[derive(Debug, Clone)]
pub struct UdsPath {
    path: std::path::PathBuf,
    uri: Uri,
}

impl PartialEq for UdsPath {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path                                             
    }
}

impl UdsPath {
                                                                                                
                                                                                                   
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }
}

                                                                       
                                                                           
                                                                                                 
                                                              
                                                              
                                                                                
                                                                       
                                                                             
                                                                             
                                                                               
                                                                           
                                                         
                                                                          
                                                                                              
   
                                                                         
                    
                         
                                   
                                                                                    
                                                                 
                                                                                              
       
                         
                                   
                                                                                        
                                                           
                                                                                          
       
                         
                                   
                                                                                   
                                                                 
                                                                                     
       
#[derive(Debug, Clone)]
pub enum Endpoint {
    Loopback(LoopbackUri),
    External(ExternalUri),
    Uds(UdsPath),
}

impl Endpoint {
                                                                             
                                                                           
                                                                     
                                                           
    pub fn parse(raw: &str) -> Result<Endpoint, EndpointError> {
        if let Some(path) = raw.strip_prefix("unix:") {
            let path = std::path::PathBuf::from(path);
            if !path.is_absolute() {
                return Err(EndpointError::UdsPathNotAbsolute);
            }
            let uri = Uri::from_static("http://localhost/v1");                                        
            return Ok(Endpoint::Uds(UdsPath { path, uri }));
        }
        let uri: Uri = raw.parse().map_err(|_| EndpointError::Invalid)?;
        match uri.scheme_str() {
            Some("http") | Some("https") => {}
            Some(_) => return Err(EndpointError::BadScheme),
            None => return Err(EndpointError::Incomplete),
        }
        let host = uri.host().ok_or(EndpointError::Incomplete)?;
        if is_literal_loopback(host) {
            Ok(Endpoint::Loopback(LoopbackUri(uri)))
        } else {
            Endpoint::external(uri)
        }
    }

                                                                                 
                                                                                   
                                                                                   
                                                                                   
                                                                          
                                                                               
                                                                                   
                                                                           
                                                         
                                                                                 
                                                    
    pub fn external(uri: Uri) -> Result<Endpoint, EndpointError> {
        if uri.scheme_str() != Some("https") {
            return Err(EndpointError::ExternalNotHttps);
        }
        let host = uri.host().ok_or(EndpointError::Incomplete)?;
        if is_literal_loopback(host) {
            return Err(EndpointError::ExternalIsLoopback);
        }
        Ok(Endpoint::External(ExternalUri(uri)))
    }

    pub fn uri(&self) -> &Uri {
        match self {
            Endpoint::Loopback(c) => &c.0,
            Endpoint::External(c) => &c.0,
            Endpoint::Uds(c) => &c.uri,
        }
    }

                                                                                            
                                                                                                        
    pub fn is_loopback(&self) -> bool {
        matches!(self, Endpoint::Loopback(_))
    }

                                                                                              
                                                                                               
                                                                                                 
                                                                                         
                                                                                               
                                                                                          
                                                                                                
                                                                                               
                                                                                              
                                                                                 
                                                                                                   
                                                               
    pub fn is_local(&self) -> bool {
        matches!(self, Endpoint::Loopback(_) | Endpoint::Uds(_))
    }
}

impl PartialEq for Endpoint {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Endpoint::Loopback(a), Endpoint::Loopback(b)) => a == b,
            (Endpoint::External(a), Endpoint::External(b)) => a == b,
            (Endpoint::Uds(a), Endpoint::Uds(b)) => a == b,
            (Endpoint::Loopback(_), _) | (Endpoint::External(_), _) | (Endpoint::Uds(_), _) => {
                false
            }
        }
    }
}

fn is_literal_loopback(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
                                                                          
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
                                                                                
                                                                          
                                                                             
                                                                              
                                                            
    bare.parse::<IpAddr>().is_ok_and(|ip| match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_loopbacks_classify_loopback() {
        for raw in [
            "http://127.0.0.1:1234/v1",
            "http://127.8.9.10/x",
            "http://[::1]:8080/search",
            "http://localhost:1234/v1",
                                                                          
            "https://[::ffff:127.0.0.1]/v1",
            "https://[::ffff:7f00:1]/v1",
            "https://[0:0:0:0:0:ffff:127.0.0.1]/v1",
        ] {
            assert!(Endpoint::parse(raw).unwrap().is_loopback(), "{raw}");
        }
    }

    #[test]
    fn arbitrary_hostnames_are_not_loopback() {
                                                                         
                                        
        assert_eq!(
            Endpoint::parse("http://localhost.attacker.example/v1"),
            Err(EndpointError::ExternalNotHttps)
        );
        assert!(matches!(
            Endpoint::parse("https://model.example/v1"),
            Ok(Endpoint::External(_))
        ));
    }

    #[test]
    fn external_constructor_rejects_literal_loopback() {
                                                                                      
                                                                                          
                                                                                   
                                                                                      
        for raw in [
            "https://127.0.0.1:8443/v1",
            "https://[::1]/x",
            "https://localhost/v1",
        ] {
            let uri: Uri = raw.parse().unwrap();
            assert_eq!(
                Endpoint::external(uri),
                Err(EndpointError::ExternalIsLoopback),
                "{raw}"
            );
        }
    }

    #[test]
    fn sealed_payloads_are_transparent_to_uri() {
                                                                                
                                                                                     
                                                                                  
                                                     
        let ep = Endpoint::parse("https://model.example/v1").unwrap();
        assert_eq!(ep.uri().to_string(), "https://model.example/v1");
        let lo = Endpoint::parse("http://127.0.0.1:1234/v1").unwrap();
        assert_eq!(lo.uri().to_string(), "http://127.0.0.1:1234/v1");
    }

    #[test]
    fn rejects_other_schemes() {
        assert_eq!(
            Endpoint::parse("ftp://127.0.0.1/x"),
            Err(EndpointError::BadScheme)
        );
        assert_eq!(
            Endpoint::parse("127.0.0.1:11434"),
            Err(EndpointError::Incomplete)
        );
    }

    #[test]
    fn unix_absolute_path_parses_to_uds() {
        let ep = Endpoint::parse("unix:/run/creatine/creatine.sock").unwrap();
        assert!(matches!(ep, Endpoint::Uds(_)));
                                                                   
        assert!(ep.is_local());
        assert!(!ep.is_loopback());
                                                                               
        assert_eq!(ep.uri().to_string(), "http://localhost/v1");
    }

    #[test]
    fn unix_relative_path_is_rejected() {
        assert_eq!(
            Endpoint::parse("unix:relative/path.sock"),
            Err(EndpointError::UdsPathNotAbsolute)
        );
    }

    #[test]
    fn uds_partial_eq() {
        let a = Endpoint::parse("unix:/run/a.sock").unwrap();
        let a2 = Endpoint::parse("unix:/run/a.sock").unwrap();
        let b = Endpoint::parse("unix:/run/b.sock").unwrap();
        let lo = Endpoint::parse("http://127.0.0.1:1234/v1").unwrap();
        let ext = Endpoint::parse("https://model.example/v1").unwrap();
        assert_eq!(a, a2);
        assert_ne!(a, b);
        assert_ne!(a, lo);
        assert_ne!(a, ext);
        assert_ne!(lo, a);
    }
}
