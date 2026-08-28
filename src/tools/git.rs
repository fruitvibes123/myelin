                                                                                
   
                                                                              
                                                                         
                                                                               
                                        
   
                                                                              
                                                                          
                                                                                
                                                            
                                                                      
                                                                           
                                                                           
                                                                                 
                                                                                 
                                                                            
                                                                               
               

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{ToolBounds, ToolError};

                                              
const GIT_POLL: Duration = Duration::from_millis(5);

const REV_RANGE_MAX_LEN: usize = 128;

                                                                             
                                                           
fn validate_rev_range(rev: &str) -> Result<(), ToolError> {
    if rev.is_empty() || rev.len() > REV_RANGE_MAX_LEN {
        return Err(ToolError::BadRevRange);
    }
    if !rev
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._/~^-".contains(&b))
    {
        return Err(ToolError::BadRevRange);
    }
    Ok(())
}

                                                                               
                                                                         
                                                                                 
                                                                              
                                                                                
                                           
const LOG_ARGS: &[&str] = &[
    "log",
    "--no-color",
    "--no-decorate",
    "--no-textconv",
    "--no-ext-diff",
    "--max-count=50",
];
const DIFF_ARGS: &[&str] = &["diff", "--no-color", "--no-textconv", "--no-ext-diff"];

pub fn git_log(
    repo: &Path,
    bounds: &ToolBounds,
    rev_range: Option<&str>,
) -> Result<String, ToolError> {
    run_git(repo, bounds, LOG_ARGS, rev_range)
}

pub fn git_diff(
    repo: &Path,
    bounds: &ToolBounds,
    rev_range: Option<&str>,
) -> Result<String, ToolError> {
    run_git(repo, bounds, DIFF_ARGS, rev_range)
}

fn run_git(
    repo: &Path,
    bounds: &ToolBounds,
    fixed_args: &[&str],
    rev_range: Option<&str>,
) -> Result<String, ToolError> {
    if let Some(rev) = rev_range {
        validate_rev_range(rev)?;
    }

    let mut cmd = Command::new("git");
    cmd.env_clear()
                                                                            
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
                                                                         
                                                                                 
                                                                                
                                                                             
                                                           
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .arg("--no-pager")
        .arg("--git-dir")
        .arg(repo.join(".git"))
        .arg("--work-tree")
        .arg(repo)
        .args(fixed_args)
        .arg("--end-of-options");
    if let Some(rev) = rev_range {
        cmd.arg(rev);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| ToolError::Git(e.to_string()))?;

    let cap = bounds.max_git_bytes;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

                                                                                  
                                                                            
                                                                                
                                                                               
                                                                               
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = stdout_pipe {
            let _ = p.take(cap.saturating_add(1)).read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = thread::spawn(move || {
        let mut kept: Vec<u8> = Vec::new();
        if let Some(mut p) = stderr_pipe {
            let mut buf = [0u8; 8192];
            loop {
                match p.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if kept.len() < 4096 {
                            let room = 4096 - kept.len();
                            kept.extend_from_slice(&buf[..n.min(room)]);
                        }
                    }
                }
            }
        }
        String::from_utf8_lossy(&kept).into_owned()
    });

                                                                                 
                                                                                
                                                                                  
                    
    let deadline = Duration::from_millis(bounds.git_deadline_ms);
    let start = Instant::now();
    let mut exit_status = None;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = Some(status);
                break;
            }
            Ok(None) => {
                if start.elapsed() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                thread::sleep(GIT_POLL);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ToolError::Git(e.to_string()));
            }
        }
    }

    if timed_out {
                                                                                 
                                                                                
                                             
        drop(out_reader);
        drop(err_reader);
        return Err(ToolError::GitTimeout(bounds.git_deadline_ms));
    }

    let mut stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    let truncated = stdout.len() as u64 > cap;
    if truncated {
        stdout.truncate(usize::try_from(cap).unwrap_or(usize::MAX));
    }

    let success = exit_status.map(|s| s.success()).unwrap_or(false);
    if !success && !truncated {
        return Err(ToolError::Git(stderr.trim().to_string()));
    }
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    if truncated {
        text.push_str(&format!("\n[truncated at {cap} bytes]\n"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rev_range_allowlist() {
        for ok in ["HEAD", "HEAD~3..HEAD", "main", "v1.0.0^2", "feature/x-y_z"] {
            assert!(validate_rev_range(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "HEAD; rm -rf /",
            "--upload-pack=/bin/sh",
            "a b",
            "rev$",
            "x:y",
            &"a".repeat(129),
        ] {
            assert!(validate_rev_range(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn both_subcommands_disable_textconv_and_ext_diff() {
        for args in [LOG_ARGS, DIFF_ARGS] {
            assert!(args.contains(&"--no-textconv"), "{args:?}");
            assert!(args.contains(&"--no-ext-diff"), "{args:?}");
        }
    }
}
