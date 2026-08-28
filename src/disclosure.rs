                                                    
   
                                                                            
                                                                              
                                                                                 
                                         
                                                        
   
                                                                   
                                                                             
                                                                      
                                                                            
                                                   
                                                                        
                                                                     
                                                           

use std::fmt;

                                                                           
                                                                               
                                                                     
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrictTrigger {
                                                                              
    Sensitive,
                                                                              
    TierB,
                                                                           
    Both,
}

impl fmt::Display for StrictTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StrictTrigger::Sensitive => "a sensitive (or undeclared)",
            StrictTrigger::TierB => "a tier_b (or undeclared)",
            StrictTrigger::Both => "a sensitive + tier_b (or undeclared)",
        })
    }
}

                                                                                
                                                                                 
                                                                                
                                               
   
                                                                            
                                                                               
                                                                           
                             
#[must_use]
pub fn classify(sensitive: Option<bool>, tier_b: Option<bool>) -> Option<StrictTrigger> {
    match (sensitive != Some(false), tier_b != Some(false)) {
        (false, false) => None,
        (true, false) => Some(StrictTrigger::Sensitive),
        (false, true) => Some(StrictTrigger::TierB),
        (true, true) => Some(StrictTrigger::Both),
    }
}

                                                                             
                                                                               
                                                                             
                                        
  
              
                                                                            
                                            
    

#[cfg(test)]
mod tests {
    use super::*;

                                                                         
                                                   
    #[test]
    fn classify_oracle_all_nine_pairings() {
        use StrictTrigger::{Both, Sensitive, TierB};
        let opts = [None, Some(true), Some(false)];
        for &s in &opts {
            for &t in &opts {
                let s_strict = s != Some(false);
                let t_strict = t != Some(false);
                let expected = match (s_strict, t_strict) {
                    (false, false) => None,
                    (true, false) => Some(Sensitive),
                    (false, true) => Some(TierB),
                    (true, true) => Some(Both),
                };
                assert_eq!(classify(s, t), expected, "classify({s:?}, {t:?})");
            }
        }
    }

                                                                               
    #[test]
    fn absent_both_is_strict_both() {
        assert_eq!(classify(None, None), Some(StrictTrigger::Both));
    }

                                                                        
    #[test]
    fn only_doubly_explicit_optout_is_non_strict() {
        assert_eq!(classify(Some(false), Some(false)), None);
                                                                            
        assert!(classify(Some(false), None).is_some());
        assert!(classify(None, Some(false)).is_some());
        assert!(classify(Some(true), Some(false)).is_some());
    }

                                                                     
    #[test]
    fn per_axis_triggers() {
        assert_eq!(
            classify(Some(true), Some(false)),
            Some(StrictTrigger::Sensitive)
        );
        assert_eq!(
            classify(Some(false), Some(true)),
            Some(StrictTrigger::TierB)
        );
        assert_eq!(classify(Some(true), Some(true)), Some(StrictTrigger::Both));
    }

                                                                                 
                                                   
    #[test]
    fn display_names_the_trigger() {
        assert_eq!(
            StrictTrigger::Sensitive.to_string(),
            "a sensitive (or undeclared)"
        );
        assert_eq!(StrictTrigger::TierB.to_string(), "a tier_b (or undeclared)");
        assert_eq!(
            StrictTrigger::Both.to_string(),
            "a sensitive + tier_b (or undeclared)"
        );
    }
}
