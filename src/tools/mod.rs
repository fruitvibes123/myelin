                                   
   
                                                                           
                                                                              
                                                                             
                                                                            
                                                                          

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

use crate::confine::{ConfineError, ConfinedPath, EntryKind, RelPath, Root};
use crate::search::SearchBackend;

pub mod git;
pub mod grep;
pub mod list;
pub mod read;

                                                                    
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCall {
    ReadFile {
        path: String,
                                         
        range: Option<(u64, u64)>,
    },
    Grep {
        pattern: String,
        path: Option<String>,
    },
    ListDir {
        path: String,
    },
                                           
    GitDiff {
        rev_range: Option<String>,
    },
                                           
    GitLog {
        rev_range: Option<String>,
    },
                                                                          
    WebSearch {
        query: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("path refused: {0}")]
    Confine(#[from] ConfineError),
    #[error("unknown tool `{0}`")]
    UnknownTool(String),
    #[error("bad arguments for `{tool}`: {msg}")]
    BadArgs { tool: &'static str, msg: String },
    #[error("path is outside the requested scope")]
    OutOfScope,
    #[error("invalid pattern: {0}")]
    BadPattern(String),
    #[error("`{0}` is not enabled for this run")]
    Disabled(&'static str),
    #[error(
        "git tools are unavailable under a restricted scope; scope globs do not apply to git output — run without scope_globs or disable git"
    )]
    GitScoped,
    #[error("rev range refused: only [A-Za-z0-9._/~^-], len ≤ 128")]
    BadRevRange,
    #[error("git: {0}")]
    Git(String),
    #[error("git exceeded the {0} ms deadline and was terminated")]
    GitTimeout(u64),
    #[error("search: {0}")]
    Search(String),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

                                                                            
                                                                           
                                                                       
#[derive(Debug, Clone, Copy, serde::Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ToolBounds {
    pub max_file_bytes: u64,
    pub max_list_entries: usize,
    pub grep_max_matches: usize,
    pub grep_max_scan_bytes: u64,
    pub grep_max_files: usize,
    pub max_walk_depth: usize,
    pub max_git_bytes: u64,
                                                                                  
                                                         
    pub git_deadline_ms: u64,
                                                                              
                                                                             
                                                                                
                                                                                   
                                                                                   
                                                                                  
                                                                                    
                                                                                    
                                                                                  
                                    
    pub grep_deadline_ms: u64,
    pub regex_size_limit: usize,
}

impl Default for ToolBounds {
    fn default() -> Self {
        ToolBounds {
            max_file_bytes: 256 * 1024,
            max_list_entries: 500,
            grep_max_matches: 100,
            grep_max_scan_bytes: 8 * 1024 * 1024,
            grep_max_files: 400,
            max_walk_depth: 32,
            max_git_bytes: 256 * 1024,
            git_deadline_ms: 30_000,
            grep_deadline_ms: 30_000,
            regex_size_limit: 1 << 20,
        }
    }
}

                                                                             
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: serde_json::Value,
}

                                                                                
                                                                               
                                                                             
                                                                              
                                                                          
                                                                               
          
#[derive(Debug, Clone)]
pub struct Scope {
    set: GlobSet,
    globs: Vec<String>,
}

impl Scope {
                                                                                
                                                                              
                          
    pub fn new(globs: Vec<String>) -> Result<Scope, globset::Error> {
        let mut builder = GlobSetBuilder::new();
        for g in &globs {
            builder.add(Glob::new(g)?);
        }
        Ok(Scope {
            set: builder.build()?,
            globs,
        })
    }

                                                      
    pub fn matcher(&self) -> &GlobSet {
        &self.set
    }

                                                                    
    pub fn globs(&self) -> &[String] {
        &self.globs
    }
}

                                                                          
                                                                            
                                                                  
                                                                             
                                                                              
                                                                                
          
pub struct Toolbox<'a> {
    pub root: &'a Root,
    pub bounds: ToolBounds,
                                                                             
                                                                             
                                                                          
                                                              
    pub scope: Option<&'a Scope>,
    pub git_enabled: bool,
    pub search: Option<&'a dyn SearchBackend>,
                                                                              
    pub excluded_dirs: &'a [String],
}

impl Toolbox<'_> {
    pub fn dispatch(&self, call: &ToolCall) -> Result<String, ToolError> {
        match call {
            ToolCall::ReadFile { path, range } => {
                let path = RelPath::normalize(path);
                self.check_scope(&path)?;
                read::read_file(self.root, &self.bounds, &path, *range)
            }
            ToolCall::Grep { pattern, path } => {
                let start = RelPath::normalize(path.as_deref().unwrap_or("."));
                grep::grep(self, pattern, &start)
            }
            ToolCall::ListDir { path } => {
                let path = RelPath::normalize(path);
                list::list_dir(self.root, &self.bounds, self.scope, &path)
            }
            ToolCall::GitDiff { rev_range } => {
                if !self.git_enabled {
                    return Err(ToolError::Disabled("git_diff"));
                }
                if self.scope.is_some() {
                    return Err(ToolError::GitScoped);
                }
                git::git_diff(self.root.label(), &self.bounds, rev_range.as_deref())
            }
            ToolCall::GitLog { rev_range } => {
                if !self.git_enabled {
                    return Err(ToolError::Disabled("git_log"));
                }
                if self.scope.is_some() {
                    return Err(ToolError::GitScoped);
                }
                git::git_log(self.root.label(), &self.bounds, rev_range.as_deref())
            }
            ToolCall::WebSearch { query } => match self.search {
                Some(backend) => crate::search::run_query(backend, query),
                None => Err(ToolError::Disabled("web_search")),
            },
        }
    }

                                                                              
                                                                                  
                                                                  
    fn check_scope(&self, path: &RelPath) -> Result<(), ToolError> {
        match self.scope {
            Some(s) if !s.matcher().is_match(path.scope_str()) => Err(ToolError::OutOfScope),
            _ => Ok(()),
        }
    }

    pub(crate) fn confined(&self, path: &RelPath) -> Result<ConfinedPath, ToolError> {
        Ok(ConfinedPath::within(self.root, path.as_path())?)
    }
}

impl ToolCall {
                                                                             
                                                                            
                                                   
    pub fn parse(name: &str, raw_args: &str) -> Result<ToolCall, ToolError> {
        let args = if raw_args.trim().is_empty() {
            "{}"
        } else {
            raw_args
        };
        match name {
            "read_file" => {
                #[derive(Deserialize)]
                struct A {
                    path: String,
                    start_line: Option<u64>,
                    end_line: Option<u64>,
                }
                let a: A = parse_args::<A>("read_file", args)?;
                let range = match (a.start_line, a.end_line) {
                    (None, None) => None,
                    (s, e) => Some((s.unwrap_or(1).max(1), e.unwrap_or(u64::MAX))),
                };
                Ok(ToolCall::ReadFile {
                    path: a.path,
                    range,
                })
            }
            "grep" => {
                #[derive(Deserialize)]
                struct A {
                    pattern: String,
                    path: Option<String>,
                }
                let a: A = parse_args::<A>("grep", args)?;
                Ok(ToolCall::Grep {
                    pattern: a.pattern,
                    path: a.path,
                })
            }
            "list_dir" => {
                #[derive(Deserialize)]
                struct A {
                    #[serde(default = "default_dot")]
                    path: String,
                }
                let a: A = parse_args::<A>("list_dir", args)?;
                Ok(ToolCall::ListDir { path: a.path })
            }
            "git_diff" | "git_log" => {
                #[derive(Deserialize)]
                struct A {
                    rev_range: Option<String>,
                }
                let a: A = parse_args::<A>("git", args)?;
                if name == "git_diff" {
                    Ok(ToolCall::GitDiff {
                        rev_range: a.rev_range,
                    })
                } else {
                    Ok(ToolCall::GitLog {
                        rev_range: a.rev_range,
                    })
                }
            }
            "web_search" => {
                #[derive(Deserialize)]
                struct A {
                    query: String,
                }
                let a: A = parse_args::<A>("web_search", args)?;
                Ok(ToolCall::WebSearch { query: a.query })
            }
            other => Err(ToolError::UnknownTool(other.to_string())),
        }
    }

                                                                              
                                                                    
    pub fn specs(git: bool, web_search: bool) -> Vec<ToolSpec> {
        let mut specs = vec![
            ToolSpec {
                name: "read_file",
                description: "Read a file inside the repository (bounded). \
                              Optionally pass start_line/end_line (1-based, inclusive).",
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path relative to the repo root" },
                        "start_line": { "type": "integer" },
                        "end_line": { "type": "integer" }
                    },
                    "required": ["path"]
                }),
            },
            ToolSpec {
                name: "grep",
                description: "Search file contents with a regex (bounded). \
                              Optional path: a file or directory to search under. \
                              The search is bounded; whenever a bound or a skip cuts it \
                              short — a skipped dot/excluded directory, a truncated directory \
                              listing, a file read only in part, an unreadable or binary entry, \
                              or a budget/deadline — the reply notes it. Symlinks are never \
                              followed (unnoted); an entry of unrecognized type is skipped \
                              with no note. Otherwise `(no matches)` means the whole search \
                              was covered. Name a path directly to search inside a skipped \
                              location.",
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Rust regex syntax" },
                        "path": { "type": "string" }
                    },
                    "required": ["pattern"]
                }),
            },
            ToolSpec {
                name: "list_dir",
                description: "List a directory inside the repository.",
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Defaults to the repo root" }
                    }
                }),
            },
        ];
        if git {
            let rev_schema = serde_json::json!({
                "type": "object",
                "properties": {
                    "rev_range": { "type": "string", "description": "e.g. HEAD~3..HEAD" }
                }
            });
            specs.push(ToolSpec {
                name: "git_log",
                description: "Show recent commit history (read-only).",
                parameters: rev_schema.clone(),
            });
            specs.push(ToolSpec {
                name: "git_diff",
                description: "Show a diff for a revision range (read-only).",
                parameters: rev_schema,
            });
        }
        if web_search {
            specs.push(ToolSpec {
                name: "web_search",
                description: "Search the web via the configured local search \
                              endpoint. Plain keyword queries only.",
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Keywords, [A-Za-z0-9 _.+-] only" }
                    },
                    "required": ["query"]
                }),
            });
        }
        specs
    }
}

fn default_dot() -> String {
    ".".to_string()
}

                               
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeProbe {
                                                           
    Matched,
                                                                        
    Empty,
                                                                               
                                                                              
                                                       
    Unknown,
}

                                                                                
                                                                           
                                                            
const SCOPE_PROBE_MAX_ENTRIES: usize = 50_000;

                                                                             
                                                                                
                                                                               
   
                                                                            
                                                                            
                                                                             
                                                                                
                                                                              
                                                                              
                                                                               
                                                                               
                                                                            
                                                                    
   
                                                                              
                                                                          
                                                                               
                                                                             
                                                                          
pub fn probe_scope(root: &Root, bounds: &ToolBounds, scope: &Scope) -> ScopeProbe {
    let Ok(cpath) = ConfinedPath::within(root, Path::new(".")) else {
        return ScopeProbe::Unknown;
    };
                                                                                
                                                                                 
    let parsed: Vec<Vec<&str>> = scope
        .globs()
        .iter()
        .map(|g| {
            g.trim_start_matches("./")
                .split('/')
                .filter(|s| !s.is_empty())
                .collect()
        })
        .collect();
    let mut walk = ScopeWalk {
        root,
        bounds,
        scope: scope.matcher(),
        globs: &parsed,
        budget: SCOPE_PROBE_MAX_ENTRIES,
        truncated: false,
    };
    if walk.walk(".", &cpath, 0) {
        ScopeProbe::Matched
    } else if walk.truncated {
        ScopeProbe::Unknown
    } else {
        ScopeProbe::Empty
    }
}

                                                                           
                                                                                
                                                                          
                                                                                
                                                                               
                                                                              
          
   
                                                                                  
                                                                           
                                                                                
                                                                                  
                                                                             
pub(crate) fn scope_admits_under(globs: &[String], dir_rel: &str) -> bool {
    let dir_segs: Vec<&str> = dir_rel
        .trim_start_matches("./")
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    globs.iter().any(|g| {
        let g_segs: Vec<&str> = g
            .trim_start_matches("./")
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        glob_could_match_under(&g_segs, &dir_segs)
    })
}

                                                                                
                                                                                
                                                                             
                                                          
   
                                                                                 
                                                                                
                                                                               
                                                                              
                                                                              
                                                                              
                                                                              
fn glob_could_match_under(glob: &[&str], rel: &[&str]) -> bool {
    for (i, r) in rel.iter().enumerate() {
        match glob.get(i) {
            None => return false,                                 
            Some(g) if is_wildcard_segment(g) => return true,                                   
            Some(g) if g == r => {}                             
            Some(_) => return false,                              
        }
    }
                                                                                  
    glob.len() > rel.len()
}

                                                                           
                                                                              
                                                                                
                                                                           
                                                                                
                                             
                                                                           
                                                                            
                                                                              
                                                                               
                                                                       
fn is_wildcard_segment(seg: &str) -> bool {
    seg.bytes()
        .any(|b| matches!(b, b'*' | b'?' | b'[' | b']' | b'{' | b'}' | b'\\'))
}

struct ScopeWalk<'a> {
    root: &'a Root,
    bounds: &'a ToolBounds,
    scope: &'a GlobSet,
                                                                       
    globs: &'a [Vec<&'a str>],
    budget: usize,
    truncated: bool,
}

impl ScopeWalk<'_> {
                                                                                 
                                                                                
                                                                         
    fn walk(&mut self, rel: &str, cpath: &ConfinedPath, depth: usize) -> bool {
        if depth >= self.bounds.max_walk_depth {
            self.truncated = true;                                                  
            return false;
        }
        let Ok(entries) = self.root.list_confined(cpath, self.bounds.max_list_entries) else {
                                                                                
                                                                               
                                                                 
            self.truncated = true;
            return false;
        };
                                                                                 
                                                                            
                                                                   
                                                                              
        if entries.len() >= self.bounds.max_list_entries {
            self.truncated = true;
        }
        for entry in entries {
            if self.budget == 0 {
                self.truncated = true;
                return false;
            }
            self.budget -= 1;
            let child_rel = if rel == "." {
                entry.name.clone()
            } else {
                format!("{}/{}", rel.trim_end_matches('/'), entry.name)
            };
            match entry.kind {
                EntryKind::Dir => {
                                                                                   
                                                                              
                                                                              
                                                                                
                    if !self.could_descend(&child_rel) {
                        continue;
                    }
                                                                                   
                                                                                   
                                                                                
                                                                                
                    let Ok(child) = ConfinedPath::within(self.root, Path::new(&child_rel)) else {
                        self.truncated = true;
                        continue;
                    };
                    if self.walk(&child_rel, &child, depth + 1) {
                        return true;
                    }
                }
                EntryKind::File => {
                    if self.scope.is_match(&child_rel) {
                        return true;
                    }
                }
                EntryKind::Other => {
                                                                              
                                                                             
                                                                             
                                                                     
                    if self.scope.is_match(&child_rel) {
                        return true;
                    }
                    if self.could_descend(&child_rel) {
                        self.truncated = true;
                    }
                }
                EntryKind::Symlink => {}                                                
            }
        }
        false
    }

                                                                               
                                                                       
    fn could_descend(&self, child_rel: &str) -> bool {
        let segs: Vec<&str> = child_rel.split('/').collect();
        self.globs.iter().any(|g| glob_could_match_under(g, &segs))
    }
}

fn parse_args<'de, T: Deserialize<'de>>(tool: &'static str, raw: &'de str) -> Result<T, ToolError> {
    serde_json::from_str(raw).map_err(|e| ToolError::BadArgs {
        tool,
        msg: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_read_file_with_range() {
        let c = ToolCall::parse("read_file", r#"{"path":"src/lib.rs","start_line":3}"#).unwrap();
        assert_eq!(
            c,
            ToolCall::ReadFile {
                path: "src/lib.rs".into(),
                range: Some((3, u64::MAX)),
            }
        );
    }

    #[test]
    fn unknown_tool_is_an_error_not_a_variant() {
        assert!(matches!(
            ToolCall::parse("write_file", r#"{"path":"x","content":"y"}"#),
            Err(ToolError::UnknownTool(_))
        ));
        assert!(matches!(
            ToolCall::parse("shell", r#"{"cmd":"rm -rf /"}"#),
            Err(ToolError::UnknownTool(_))
        ));
    }

    #[test]
    fn specs_omit_disabled_capabilities() {
        let names: Vec<_> = ToolCall::specs(false, false)
            .iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["read_file", "grep", "list_dir"]);
        let names: Vec<_> = ToolCall::specs(true, true).iter().map(|s| s.name).collect();
        assert!(names.contains(&"git_log") && names.contains(&"web_search"));
    }

    fn probe(root: &Root, bounds: &ToolBounds, globs: &[&str]) -> ScopeProbe {
        let owned: Vec<String> = globs.iter().map(|s| s.to_string()).collect();
        let scope = Scope::new(owned).unwrap();
        probe_scope(root, bounds, &scope)
    }

    #[test]
    fn probe_scope_matches_reachable_files_including_dot_and_excluded_dirs() {
                                                                     
                                                                                
                                                                               
                                                                                 
                                                  
        let tmp = tempfile::tempdir().unwrap();
        for dir in [".github", "target", "src"] {
            std::fs::create_dir(tmp.path().join(dir)).unwrap();
        }
        std::fs::write(tmp.path().join(".github/ci.yml"), "on: push\n").unwrap();
        std::fs::write(tmp.path().join("target/gen.rs"), "// generated\n").unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "fn x() {}\n").unwrap();
        let root = Root::open(tmp.path()).unwrap();
        let b = ToolBounds::default();

        for scope in [".github/**", "target/**", "src/**", "**/*.rs"] {
            assert_eq!(
                probe(&root, &b, &[scope]),
                ScopeProbe::Matched,
                "scope {scope} resolves to a reachable file but probed as non-Matched",
            );
        }
                                                                    
        assert_eq!(probe(&root, &b, &["does/not/exist/**"]), ScopeProbe::Empty);
    }

    #[test]
    fn probe_scope_descends_for_separator_spanning_wildcards() {
                                                                               
                                                                                    
                                                                              
                                                                            
                                                                         
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/sub")).unwrap();
        std::fs::write(tmp.path().join("src/x.rs"), "fn x() {}\n").unwrap();
        std::fs::write(tmp.path().join("src/sub/foo.rs"), "fn f() {}\n").unwrap();
        let root = Root::open(tmp.path()).unwrap();
        let b = ToolBounds::default();
        for scope in ["*.rs", "src/*.rs", "src?x.rs", "src/[a-z]*.rs"] {
            assert_eq!(
                probe(&root, &b, &[scope]),
                ScopeProbe::Matched,
                "scope {scope} matches a reachable file but probed as non-Matched",
            );
        }
    }

    #[test]
    fn probe_scope_descends_for_escaped_literal_segments() {
                                                                           
                                                                         
                                                                          
                                                                                     
                                                                               
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("foo-bar")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "fn x() {}\n").unwrap();
        std::fs::write(tmp.path().join("foo-bar/x.rs"), "fn y() {}\n").unwrap();
        let root = Root::open(tmp.path()).unwrap();
        let b = ToolBounds::default();
        for scope in [r"sr\c/lib.rs", r"foo\-bar/x.rs", r"sr\c/*.rs"] {
            assert_eq!(
                probe(&root, &b, &[scope]),
                ScopeProbe::Matched,
                "escaped-literal scope {scope} matches a reachable file but probed as non-Matched",
            );
        }
    }

    #[test]
    fn probe_scope_prunes_unmatched_subtrees_for_fast_fail() {
                                                                                
                                                                                 
                                                                               
                                                                               
                                                                               
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("deep/a/b/c")).unwrap();
        std::fs::write(tmp.path().join("deep/a/b/c/x.rs"), "x\n").unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "fn x() {}\n").unwrap();
        let root = Root::open(tmp.path()).unwrap();
        let bounds = ToolBounds {
            max_walk_depth: 2,
            ..ToolBounds::default()
        };
                                                                                
        assert_eq!(
            probe(&root, &bounds, &["no/such/dir/**"]),
            ScopeProbe::Empty
        );
        assert_eq!(probe(&root, &bounds, &["src/nope.rs"]), ScopeProbe::Empty);
                                                                               
                                                                            
        assert_eq!(probe(&root, &bounds, &["deep/**"]), ScopeProbe::Unknown);
    }

    #[test]
    fn probe_scope_does_not_false_empty_when_a_listing_is_truncated() {
                                                                                 
                                                                                
                                                                                 
                                                                          
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("data")).unwrap();
        for n in ["a", "b", "c", "d", "match"] {
            std::fs::write(tmp.path().join(format!("data/{n}.rs")), "x\n").unwrap();
        }
        let root = Root::open(tmp.path()).unwrap();
        let bounds = ToolBounds {
            max_list_entries: 2,
            ..ToolBounds::default()
        };
        assert_ne!(probe(&root, &bounds, &["data/match.rs"]), ScopeProbe::Empty);
    }

    #[test]
    fn probe_scope_finds_a_deeply_nested_match() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("a/b/c")).unwrap();
        std::fs::write(tmp.path().join("a/b/c/deep.rs"), "fn d() {}\n").unwrap();
        let root = Root::open(tmp.path()).unwrap();
        assert_eq!(
            probe(&root, &ToolBounds::default(), &["a/b/c/*.rs"]),
            ScopeProbe::Matched,
        );
    }
}
