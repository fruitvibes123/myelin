                                                               
                                                                             
                                                                               
                            

use std::path::Path;
use std::time::{Duration, Instant};

use regex::RegexBuilder;

use crate::confine::{ConfinedPath, EntryKind, RelPath, Root, escape_control_chars};

use super::{ToolBounds, ToolError, Toolbox};

pub(crate) fn grep(tb: &Toolbox<'_>, pattern: &str, start: &RelPath) -> Result<String, ToolError> {
    let re = RegexBuilder::new(pattern)
        .size_limit(tb.bounds.regex_size_limit)
        .dfa_size_limit(tb.bounds.regex_size_limit)
        .build()
        .map_err(|e| ToolError::BadPattern(e.to_string()))?;

    let arg = start.scope_str();

                                                                            
                                                                                
                                                                                  
                                                                                
                                                              
    if let Some(s) = tb.scope
        && !s.matcher().is_match(arg)
        && !super::scope_admits_under(s.globs(), arg)
    {
        return Err(ToolError::OutOfScope);
    }

    let cpath = tb.confined(start)?;

    let mut walk = Walk {
        root: tb.root,
        bounds: &tb.bounds,
        scope: tb.scope.map(|s| s.matcher()),
        scope_globs: tb.scope.map(|s| s.globs()),
        excluded: tb.excluded_dirs,
        re,
        scanned_bytes: 0,
        files_visited: 0,
        matches: Vec::new(),
        notes: WalkNotes::default(),
        deadline: Instant::now() + Duration::from_millis(tb.bounds.grep_deadline_ms),
    };

                                                        
    match tb
        .root
        .read_confined(&cpath, tb.bounds.max_file_bytes.saturating_add(1))
    {
        Ok(mut bytes) => {
                                                                          
                                                                             
            let rel = arg;
            if let Some(s) = tb.scope
                && !s.matcher().is_match(rel)
            {
                return Err(ToolError::OutOfScope);
            }
                                                                                   
                                                                                   
            if bytes.len() as u64 > tb.bounds.max_file_bytes {
                walk.notes.files_truncated += 1;
                bytes.truncate(usize::try_from(tb.bounds.max_file_bytes).unwrap_or(usize::MAX));
            }
            walk.scan_file(rel, &bytes);
        }
        Err(crate::confine::ConfineError::NotRegularFile) => walk.walk_dir(arg, &cpath, 0)?,
        Err(e) => return Err(e.into()),
    }

    let mut out = String::new();
    for line in &walk.matches {
        out.push_str(line);
        out.push('\n');
    }
    if walk.matches.is_empty() {
        out.push_str("(no matches)\n");
    }
    walk.render_notes(&mut out);
    Ok(out)
}

                                                                               
                                                                             
                                                                                
                                                                                 
                                                                           
                                           
#[derive(Default)]
struct WalkNotes {
                                                                  
    capped: bool,
                                                  
    skipped_dirs: usize,
                                                                                    
    list_truncated: bool,
                                                              
    files_truncated: usize,
                                                                            
    unreadable: usize,
                                                       
    binary_skipped: usize,
                                                                                
                                                                                   
                                                              
    unlistable_dirs: usize,
}

struct Walk<'a> {
    root: &'a Root,
    bounds: &'a ToolBounds,
    scope: Option<&'a globset::GlobSet>,
                                                                            
                                                                              
                                                                             
    scope_globs: Option<&'a [String]>,
    excluded: &'a [String],
    re: regex::Regex,
    scanned_bytes: u64,
    files_visited: usize,
    matches: Vec<String>,
    notes: WalkNotes,
    deadline: Instant,
}

impl Walk<'_> {
    fn over_budget(&self) -> bool {
        self.matches.len() >= self.bounds.grep_max_matches
            || self.scanned_bytes >= self.bounds.grep_max_scan_bytes
            || self.files_visited >= self.bounds.grep_max_files
            || Instant::now() >= self.deadline
    }

                                                                                
                                                            
    fn could_descend(&self, child_rel: &str) -> bool {
        match self.scope_globs {
            Some(globs) => super::scope_admits_under(globs, child_rel),
            None => true,
        }
    }

    fn walk_dir(&mut self, rel: &str, cpath: &ConfinedPath, depth: usize) -> Result<(), ToolError> {
        if depth >= self.bounds.max_walk_depth {
            self.notes.capped = true;
            return Ok(());
        }
                                                                               
                                                                                
                                                        
                                                                                  
                                                                                
                                                                                  
        let mut entries = match self
            .root
            .list_confined(cpath, self.bounds.max_list_entries.saturating_add(1))
        {
            Ok(e) => e,
            Err(_) if depth > 0 => {
                self.notes.unlistable_dirs += 1;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        if entries.len() > self.bounds.max_list_entries {
            self.notes.list_truncated = true;
            entries.truncate(self.bounds.max_list_entries);
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        for entry in entries {
            if self.over_budget() {
                self.notes.capped = true;
                return Ok(());
            }
            let child_rel = if rel.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", rel.trim_end_matches('/'), entry.name)
            };
            match entry.kind {
                EntryKind::Dir => {
                                                                                   
                                                                                 
                                                                             
                    if !self.could_descend(&child_rel) {
                        continue;
                    }
                    if entry.name.starts_with('.') || self.excluded.contains(&entry.name) {
                        self.notes.skipped_dirs += 1;
                        continue;
                    }
                                                                                  
                                                                       
                    let Ok(child) = ConfinedPath::within(self.root, Path::new(&child_rel)) else {
                        self.notes.unreadable += 1;
                        continue;
                    };
                    self.walk_dir(&child_rel, &child, depth + 1)?;
                }
                EntryKind::File => {
                    if let Some(set) = self.scope
                        && !set.is_match(&child_rel)
                    {
                        continue;
                    }
                    let Ok(child) = ConfinedPath::within(self.root, Path::new(&child_rel)) else {
                        self.notes.unreadable += 1;
                        continue;
                    };
                    match self
                        .root
                        .read_confined(&child, self.bounds.max_file_bytes.saturating_add(1))
                    {
                        Ok(mut bytes) => {
                            if bytes.len() as u64 > self.bounds.max_file_bytes {
                                self.notes.files_truncated += 1;
                                bytes.truncate(
                                    usize::try_from(self.bounds.max_file_bytes)
                                        .unwrap_or(usize::MAX),
                                );
                            }
                            self.scan_file(&child_rel, &bytes);
                        }
                        Err(_) => {
                            self.notes.unreadable += 1;
                            continue;                                                          
                        }
                    }
                }
                                                                       
                                                                           
                EntryKind::Symlink | EntryKind::Other => {}
            }
        }
        Ok(())
    }

                                                                               
                                                                 
    fn render_notes(&self, out: &mut String) {
        if self.notes.capped {
            out.push_str(
                "[stopped early: a grep bound was reached — narrow the pattern or path]\n",
            );
        }
        if self.notes.skipped_dirs > 0 {
            out.push_str(&format!(
                "[skipped {} dot/excluded {}; name a path to search them]\n",
                self.notes.skipped_dirs,
                if self.notes.skipped_dirs == 1 {
                    "directory"
                } else {
                    "directories"
                },
            ));
        }
        if self.notes.list_truncated {
            out.push_str(&format!(
                "[a directory listing was truncated at {} entries; entries past the cap were not searched — name a narrower path]\n",
                self.bounds.max_list_entries,
            ));
        }
        if self.notes.files_truncated > 0 {
            out.push_str(&format!(
                "[{} {} read only to the first {} bytes; a match past that offset would be missed]\n",
                self.notes.files_truncated,
                if self.notes.files_truncated == 1 {
                    "file was"
                } else {
                    "files were"
                },
                self.bounds.max_file_bytes,
            ));
        }
        if self.notes.unreadable > 0 {
            out.push_str(&format!(
                "[{} {} could not be read and {} skipped]\n",
                self.notes.unreadable,
                if self.notes.unreadable == 1 {
                    "entry"
                } else {
                    "entries"
                },
                if self.notes.unreadable == 1 {
                    "was"
                } else {
                    "were"
                },
            ));
        }
        if self.notes.binary_skipped > 0 {
            out.push_str(&format!(
                "[{} binary {} skipped]\n",
                self.notes.binary_skipped,
                if self.notes.binary_skipped == 1 {
                    "file"
                } else {
                    "files"
                },
            ));
        }
        if self.notes.unlistable_dirs > 0 {
            out.push_str(&format!(
                "[{} {} could not be listed and {} skipped; name a path to search elsewhere]\n",
                self.notes.unlistable_dirs,
                if self.notes.unlistable_dirs == 1 {
                    "directory"
                } else {
                    "directories"
                },
                if self.notes.unlistable_dirs == 1 {
                    "was"
                } else {
                    "were"
                },
            ));
        }
    }

    fn scan_file(&mut self, rel: &str, bytes: &[u8]) {
        self.files_visited += 1;
        self.scanned_bytes += bytes.len() as u64;
                                                                               
        if bytes.iter().take(4096).any(|b| *b == 0) {
            self.notes.binary_skipped += 1;
            return;
        }
                                                                                
                                                                                   
        let rel_display = escape_control_chars(rel);
        let text = String::from_utf8_lossy(bytes);
        for (idx, line) in text.lines().enumerate() {
            if self.matches.len() >= self.bounds.grep_max_matches {
                self.notes.capped = true;
                return;
            }
            if Instant::now() >= self.deadline {
                self.notes.capped = true;
                return;
            }
            if self.re.is_match(line) {
                let shown: String = line.chars().take(400).collect();
                self.matches
                    .push(format!("{rel_display}:{}: {shown}", idx + 1));
            }
        }
    }
}
