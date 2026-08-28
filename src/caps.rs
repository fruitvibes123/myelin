                                                              
                                                                             
                                                                           
              

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

                                                                          
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Done,
    CapIterations,
    CapWallclock,
    CapToolCalls,
    CapOutput,
    Error,
                                                                            
                                                                          
                                                                           
                 
    EmptyOutput,
                                                                             
                       
    EmptyScope,
}

                                                                             
                                     
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CallCaps {
                                           
    pub max_iterations: u32,
                                            
    pub max_tool_calls: u32,
                                                              
                                                                                  
                                                                                  
                                                                                
                                                                                   
                                                                                
                                                      
    pub wall_clock_ms: u64,
                                                                   
    pub max_output_bytes: u64,
}

impl Default for CallCaps {
    fn default() -> Self {
                                                                             
                                                                        
                                                                           
                         
        CallCaps {
            max_iterations: 24,
            max_tool_calls: 96,
            wall_clock_ms: 300_000,
            max_output_bytes: 2 << 20,
        }
    }
}

                                                                            
                                                                              
                                                   
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SessionId(pub u64);

#[derive(Debug)]
struct Counter {
    used: AtomicU64,
    limit: u64,
}

impl Counter {
    fn new(limit: u64) -> Counter {
        Counter {
            used: AtomicU64::new(0),
            limit,
        }
    }

                                                                        
                                                            
    fn charge(&self, n: u64) -> bool {
        let prev = self.used.fetch_add(n, Ordering::Relaxed);
        prev.saturating_add(n) <= self.limit
    }
}

                                                                      
                                                                    
                                                                   
                                                                            
                                         
#[derive(Debug)]
pub struct SessionBudget {
    iterations: Counter,
    tool_calls: Counter,
    output_bytes: Counter,
}

impl SessionBudget {
    pub fn new(caps: &CallCaps, multiplier: u32) -> SessionBudget {
        let m = u64::from(multiplier.max(1));
        SessionBudget {
            iterations: Counter::new(u64::from(caps.max_iterations).saturating_mul(m)),
            tool_calls: Counter::new(u64::from(caps.max_tool_calls).saturating_mul(m)),
            output_bytes: Counter::new(caps.max_output_bytes.saturating_mul(m)),
        }
    }
}

                                                                           
                                                                            
pub struct CallBudget<'s> {
    caps: CallCaps,
    session: &'s SessionBudget,
    started: Instant,
    iterations: u32,
    tool_calls: u32,
    output_bytes: u64,
}

impl<'s> CallBudget<'s> {
    pub fn new(caps: CallCaps, session: &'s SessionBudget) -> CallBudget<'s> {
        CallBudget {
            caps,
            session,
            started: Instant::now(),
            iterations: 0,
            tool_calls: 0,
            output_bytes: 0,
        }
    }

                                                                      
                                                                         
                  
    pub fn begin_iteration(&mut self) -> Result<(), StopReason> {
        self.check_wallclock()?;
        self.iterations += 1;
        if self.iterations > self.caps.max_iterations || !self.session.iterations.charge(1) {
            return Err(StopReason::CapIterations);
        }
        Ok(())
    }

    pub fn charge_tool_call(&mut self) -> Result<(), StopReason> {
        self.tool_calls += 1;
        if self.tool_calls > self.caps.max_tool_calls || !self.session.tool_calls.charge(1) {
            return Err(StopReason::CapToolCalls);
        }
        Ok(())
    }

    pub fn charge_output(&mut self, bytes: u64) -> Result<(), StopReason> {
        self.output_bytes = self.output_bytes.saturating_add(bytes);
        if self.output_bytes > self.caps.max_output_bytes
            || !self.session.output_bytes.charge(bytes)
        {
            return Err(StopReason::CapOutput);
        }
        Ok(())
    }

    pub fn check_wallclock(&self) -> Result<(), StopReason> {
        if self.started.elapsed() >= Duration::from_millis(self.caps.wall_clock_ms) {
            return Err(StopReason::CapWallclock);
        }
        Ok(())
    }

    pub fn iterations(&self) -> u32 {
        self.iterations
    }

    pub fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(it: u32, tc: u32, ms: u64, out: u64) -> CallCaps {
        CallCaps {
            max_iterations: it,
            max_tool_calls: tc,
            wall_clock_ms: ms,
            max_output_bytes: out,
        }
    }

    #[test]
    fn iteration_cap_fires() {
        let c = caps(2, 10, 60_000, 1 << 20);
        let session = SessionBudget::new(&c, 10);
        let mut b = CallBudget::new(c, &session);
        assert!(b.begin_iteration().is_ok());
        assert!(b.begin_iteration().is_ok());
        assert_eq!(b.begin_iteration(), Err(StopReason::CapIterations));
    }

    #[test]
    fn tool_call_cap_fires() {
        let c = caps(10, 1, 60_000, 1 << 20);
        let session = SessionBudget::new(&c, 10);
        let mut b = CallBudget::new(c, &session);
        assert!(b.charge_tool_call().is_ok());
        assert_eq!(b.charge_tool_call(), Err(StopReason::CapToolCalls));
    }

    #[test]
    fn output_cap_fires_and_is_sticky() {
        let c = caps(10, 10, 60_000, 100);
        let session = SessionBudget::new(&c, 10);
        let mut b = CallBudget::new(c, &session);
        assert!(b.charge_output(60).is_ok());
        assert_eq!(b.charge_output(60), Err(StopReason::CapOutput));
        assert_eq!(b.charge_output(1), Err(StopReason::CapOutput));
    }

    #[test]
    fn zero_wallclock_trips_immediately() {
        let c = caps(10, 10, 0, 1 << 20);
        let session = SessionBudget::new(&c, 10);
        let mut b = CallBudget::new(c, &session);
        assert_eq!(b.begin_iteration(), Err(StopReason::CapWallclock));
    }

    #[test]
    fn session_budget_exhausts_across_calls() {
        let c = caps(2, 10, 60_000, 1 << 20);
        let session = SessionBudget::new(&c, 2);                      
        for _ in 0..2 {
            let mut b = CallBudget::new(c, &session);
            assert!(b.begin_iteration().is_ok());
            assert!(b.begin_iteration().is_ok());
        }
        let mut b = CallBudget::new(c, &session);
        assert_eq!(b.begin_iteration(), Err(StopReason::CapIterations));
    }
}
