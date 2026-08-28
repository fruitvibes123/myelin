                                                      
   
                                                                          
                                                                          
                                                                

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::caps::CallCaps;
use crate::tools::ToolBounds;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
                                                                              
                                                                                  
                                                                                
                                                                              
    #[serde(default)]
    pub root: Option<PathBuf>,
                                                              
    pub model_endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
                                                                                  
                                                                                   
    #[serde(default)]
    pub model_pin: Option<String>,
                                                                              
                                                                            
                                                                              
                                                                        
                                                                             
                                                                        
                                                                             
                                                                                
                                                                 
                                                                              
                                                                              
                           
    #[serde(
        default = "default_model_timeout_secs",
        deserialize_with = "bounded_model_timeout"
    )]
    pub model_timeout_secs: u64,
                                                               
    #[serde(default)]
    pub search: Option<SearchConfig>,
    #[serde(default)]
    pub caps: CallCaps,
    #[serde(default)]
    pub bounds: ToolBounds,
                                                         
    #[serde(default = "default_true")]
    pub git: bool,
                                                         
    #[serde(default = "default_session_multiplier")]
    pub session_multiplier: u32,
                                                                           
                                                               
    #[serde(default = "default_excluded_dirs")]
    pub excluded_dirs: Vec<String>,
                                                                             
                                         
                                                                            
                                                                      
    #[serde(default)]
    pub default_scope: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
                                                                          
    pub endpoint: String,
                                                              
    #[serde(default)]
    pub pin: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config {path}: {err}")]
    Io {
        path: PathBuf,
        #[source]
        err: std::io::Error,
    },
    #[error("config {0} must be owner-only — run: chmod 600 {0}")]
    Permissions(PathBuf),
    #[error("config {path} did not parse: {err}")]
    Parse {
        path: PathBuf,
        #[source]
        err: serde_json::Error,
    },
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let io_err = |err| ConfigError::Io {
        path: path.to_path_buf(),
        err,
    };
    let meta = std::fs::metadata(path).map_err(io_err)?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::Permissions(path.to_path_buf()));
    }
    let text = std::fs::read_to_string(path).map_err(io_err)?;
    serde_json::from_str(&text).map_err(|err| ConfigError::Parse {
        path: path.to_path_buf(),
        err,
    })
}

fn default_model() -> String {
                                                                           
    "local-model".to_string()
}

fn default_true() -> bool {
    true
}

fn default_session_multiplier() -> u32 {
    8
}

fn default_model_timeout_secs() -> u64 {
                                                                            
    300
}

fn bounded_model_timeout<'de, D>(d: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(d)?;
    if secs == 0 {
        return Err(serde::de::Error::custom(
            "model_timeout_secs must be ≥ 1 — 0 would time out every model request instantly",
        ));
    }
    if secs > crate::inference::MAX_MODEL_TIMEOUT_SECS {
        return Err(serde::de::Error::custom(format_args!(
            "model_timeout_secs must be ≤ {} (24 h) — one generation never legitimately runs \
             longer, and absurd values overflow the HTTP deadline arithmetic",
            crate::inference::MAX_MODEL_TIMEOUT_SECS
        )));
    }
    Ok(secs)
}

fn default_excluded_dirs() -> Vec<String> {
    ["target", "node_modules", "dist", "build"]
        .map(String::from)
        .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_config(dir: &Path, mode: u32) -> PathBuf {
        let path = dir.join("config.json");
        fs::write(
            &path,
            serde_json::json!({
                "root": dir,
                "model_endpoint": "http://127.0.0.1:1234/v1"
            })
            .to_string(),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn refuses_group_or_other_readable_config() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(tmp.path(), 0o644);
        assert!(matches!(load(&path), Err(ConfigError::Permissions(_))));
        let path = write_config(tmp.path(), 0o660);
        assert!(matches!(load(&path), Err(ConfigError::Permissions(_))));
    }

    #[test]
    fn loads_owner_only_config_with_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(tmp.path(), 0o600);
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.model, "local-model");
        assert!(cfg.git);
        assert!(cfg.search.is_none());
        assert_eq!(cfg.session_multiplier, 8);
        assert_eq!(cfg.model_timeout_secs, 300);
        assert!(cfg.root.is_some());
        assert!(cfg.default_scope.is_empty());
    }

    #[test]
    fn root_is_optional_for_global_configs() {
                                                                              
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("global.json");
        fs::write(&path, r#"{ "model_endpoint": "http://127.0.0.1:1234/v1" }"#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let cfg = load(&path).unwrap();
        assert!(cfg.root.is_none());
    }

    #[test]
    fn model_timeout_parses_and_zero_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.json");
        fs::write(
            &path,
            r#"{ "model_endpoint": "http://127.0.0.1:1/v1", "model_timeout_secs": 900 }"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(load(&path).unwrap().model_timeout_secs, 900);
                                                                                   
                                                            
        fs::write(
            &path,
            r#"{ "model_endpoint": "http://127.0.0.1:1/v1", "model_timeout_secs": 0 }"#,
        )
        .unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn model_timeout_above_the_ceiling_is_refused() {
                                                                                 
                                                                                   
                                                                                    
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.json");
        let max = crate::inference::MAX_MODEL_TIMEOUT_SECS;
        for (secs, ok) in [(max, true), (max + 1, false), (u64::MAX, false)] {
            fs::write(
                &path,
                format!(
                    r#"{{ "model_endpoint": "http://127.0.0.1:1/v1", "model_timeout_secs": {secs} }}"#
                ),
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            match load(&path) {
                Ok(cfg) => assert!(
                    ok && cfg.model_timeout_secs == max,
                    "{secs} must load as max"
                ),
                Err(e) => assert!(
                    !ok && matches!(e, ConfigError::Parse { .. }),
                    "{secs} must be a parse refusal, got {e}"
                ),
            }
        }
    }

    #[test]
    fn unknown_fields_fail_loudly() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.json");
        fs::write(
            &path,
            r#"{ "root": "/x", "model_endpoint": "http://127.0.0.1:1/v1", "allow_writes": true }"#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Parse { .. })));
    }

    #[test]
    fn nested_unknown_fields_fail_loudly() {
                                                                                 
                                                                                                          
        let tmp = tempfile::tempdir().unwrap();
        for body in [
            r#"{ "root": "/x", "model_endpoint": "http://127.0.0.1:1/v1",
                "bounds": { "max_file_bytes": 1, "rogue_nested_key": true } }"#,
            r#"{ "root": "/x", "model_endpoint": "http://127.0.0.1:1/v1",
                "caps": { "max_iterations": 1, "rogue_nested_key": true } }"#,
        ] {
            let path = tmp.path().join("nested.json");
            fs::write(&path, body).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(
                matches!(load(&path), Err(ConfigError::Parse { .. })),
                "a nested unknown key must be refused"
            );
        }
    }
}
