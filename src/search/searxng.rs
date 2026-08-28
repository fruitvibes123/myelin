                                                                          
                                                                               
                                                                              
                                                                                
                                                                              
                                                                          

use std::time::Duration;

use crate::confine::escape_control_chars;
use crate::endpoint::Endpoint;
use crate::tls::{AgentKind, SpkiPin, pinned_agent, plain_agent, select};

use super::{SearchBackend, SearchError};

pub struct SearxngBackend {
    agent: ureq::Agent,
                                                                         
    base: String,
    timeout: Duration,
    max_response_bytes: u64,
    max_results: usize,
}

impl SearxngBackend {
    pub fn new(endpoint: &Endpoint, pin: Option<SpkiPin>) -> Result<SearxngBackend, SearchError> {
                                                                                             
                                                                                       
        if let Endpoint::Uds(_) = endpoint {
            return Err(SearchError::UdsNotSupported);
        }
                                                                                             
        let agent = match select(endpoint, false, pin.is_some()) {
            AgentKind::Plain => plain_agent(),
                                                                                          
                                               
            AgentKind::ServerPin | AgentKind::Mtls => {
                pinned_agent(pin.ok_or(SearchError::PinRequired)?)?
            }
        };
        Ok(SearxngBackend {
            agent,
            base: endpoint.uri().to_string(),
            timeout: Duration::from_secs(10),
            max_response_bytes: 512 * 1024,
            max_results: 5,
        })
    }
}

impl SearchBackend for SearxngBackend {
    fn search(&self, query: &str) -> Result<String, SearchError> {
                                                                              
        let sep = if self.base.contains('?') { '&' } else { '?' };
        let url = format!("{}{sep}q={}&format=json", self.base, encode_query(query));

        let mut response = self
            .agent
            .get(&url)
            .config()
            .timeout_global(Some(self.timeout))
            .build()
            .call()
            .map_err(|e| SearchError::Http(e.to_string()))?;
        let body = response
            .body_mut()
            .with_config()
            .limit(self.max_response_bytes)
            .read_to_string()
            .map_err(|e| SearchError::Http(e.to_string()))?;

        let json: serde_json::Value =
            serde_json::from_str(&body).map_err(|_| SearchError::BadResponse)?;
        let results = json["results"].as_array().ok_or(SearchError::BadResponse)?;

                                                                           
                                                                      
        let mut out = String::new();
        for (i, r) in results.iter().take(self.max_results).enumerate() {
                                                                               
                                                                                 
                                                                              
            let title = escape_control_chars(r["title"].as_str().unwrap_or("(untitled)"));
            let snippet = escape_control_chars(r["content"].as_str().unwrap_or(""));
            out.push_str(&format!("{}. {title}\n   {snippet}\n", i + 1));
        }
        if out.is_empty() {
            out.push_str("(no results)\n");
        }
        Ok(out)
    }
}

                                                                           
                                                                    
fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for b in query.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => out.push(b as char),
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_space_and_plus() {
        assert_eq!(encode_query("a b+c_d.e-f"), "a%20b%2Bc_d.e-f");
    }

    #[test]
    fn uds_endpoint_is_refused_not_misrouted() {
        let ep = Endpoint::parse("unix:/run/creatine/creatine.sock").unwrap();
        assert!(matches!(
            SearxngBackend::new(&ep, None),
            Err(SearchError::UdsNotSupported)
        ));
    }
}
