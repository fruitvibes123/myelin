                                                                          
                                                     

use crate::confine::{ConfinedPath, EntryKind, RelPath, Root};

use super::{Scope, ToolBounds, ToolError};

pub(crate) fn list_dir(
    root: &Root,
    bounds: &ToolBounds,
    scope: Option<&Scope>,
    path: &RelPath,
) -> Result<String, ToolError> {
    let base = path.scope_str();

                                                                          
                                                                            
                                                                            
                                                                                  
                                                
    if let Some(s) = scope
        && !s.matcher().is_match(base)
        && !super::scope_admits_under(s.globs(), base)
    {
        return Err(ToolError::OutOfScope);
    }

    let cpath = ConfinedPath::within(root, path.as_path())?;
    let mut entries = root.list_confined(&cpath, bounds.max_list_entries)?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));

                                                                          
                                                                              
                                                                               
                                                                             
                                                                               
                                                                          
                   
    let mut out = String::new();
    let mut shown = 0usize;
    for e in &entries {
        if let Some(s) = scope {
            let rel = if base.is_empty() {
                e.name.clone()
            } else {
                format!("{base}/{}", e.name)
            };
            let visible = if e.kind == EntryKind::Dir {
                super::scope_admits_under(s.globs(), &rel)
            } else {
                s.matcher().is_match(&rel)
            };
            if !visible {
                continue;
            }
        }
        let tag = match e.kind {
            EntryKind::Dir => "dir ",
            EntryKind::File => "file",
            EntryKind::Symlink => "link",
            EntryKind::Other => "other",
        };
        out.push_str(&format!("{tag}  {}\n", e.display_name()));
        shown += 1;
    }
    if entries.is_empty() {
        out.push_str("(empty directory)\n");
    } else if shown == 0 {
        out.push_str("(no entries in scope)\n");
    }
                                                                               
                                                                            
                                                                                 
                   
    if entries.len() >= bounds.max_list_entries {
        out.push_str(&format!(
            "[truncated at {} entries]\n",
            bounds.max_list_entries
        ));
    }
    Ok(out)
}
