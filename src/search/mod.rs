                                                            
                                                                              
                                                                
                                                                             
                

use crate::tools::ToolError;

#[cfg(feature = "net")]
pub mod searxng;

#[cfg(feature = "net")]
pub use searxng::SearxngBackend;

                                                                            
                                                                              
pub trait SearchBackend: Send + Sync {
    fn search(&self, query: &str) -> Result<String, SearchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("query refused: chars [A-Za-z0-9 _.+-] only, length 1..={QUERY_MAX_LEN}")]
    BadQuery,
    #[error("external search endpoint requires a key pin")]
    PinRequired,
    #[error("search request failed: {0}")]
    Http(String),
    #[error("search response was not the expected JSON shape")]
    BadResponse,
    #[error("a unix-socket endpoint is not supported for search backends")]
    UdsNotSupported,
    #[cfg(feature = "net")]
    #[error(transparent)]
    Tls(#[from] crate::tls::TlsError),
}

pub const QUERY_MAX_LEN: usize = 256;

                                                                           
                                                                         
                                                                           
                            
pub fn validate_query(query: &str) -> Result<(), SearchError> {
    if query.is_empty() || query.len() > QUERY_MAX_LEN {
        return Err(SearchError::BadQuery);
    }
    if !query
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b" _.+-".contains(&b))
    {
        return Err(SearchError::BadQuery);
    }
    Ok(())
}

                                                                  
pub fn run_query(backend: &dyn SearchBackend, query: &str) -> Result<String, ToolError> {
    validate_query(query).map_err(|e| ToolError::Search(e.to_string()))?;
    backend
        .search(query)
        .map_err(|e| ToolError::Search(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_keyword_queries() {
        for q in [
            "rust openat2 docs",
            "tokio spawn_blocking",
            "x86-64 abi v1.0",
        ] {
            assert!(validate_query(q).is_ok(), "{q}");
        }
    }

    #[test]
    fn rejects_urls_and_shell_chars() {
        for q in [
            "site:evil.example/payload",
            "https://evil.example/x",
            "a;rm -rf",
            "q&format=csv",
            "x?y",
            "\"quoted\"",
            "",
        ] {
            assert!(validate_query(q).is_err(), "{q}");
        }
        assert!(validate_query(&"a".repeat(QUERY_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn run_query_validates_before_dispatch() {
                                                                                                     
                                                                                                           
                                                                                                       
        use std::sync::atomic::{AtomicBool, Ordering};
        struct SpyBackend<'a>(&'a AtomicBool);
        impl SearchBackend for SpyBackend<'_> {
            fn search(&self, _q: &str) -> Result<String, SearchError> {
                self.0.store(true, Ordering::SeqCst);
                Ok("reached".to_string())
            }
        }
        let called = AtomicBool::new(false);
        let backend = SpyBackend(&called);
                                                                                                          
        assert!(run_query(&backend, "site:evil.example/x").is_err());
        assert!(
            !called.load(Ordering::SeqCst),
            "an invalid query must never reach the backend"
        );
                                                                                                             
        assert_eq!(run_query(&backend, "valid keyword").unwrap(), "reached");
        assert!(called.load(Ordering::SeqCst));
    }
}
