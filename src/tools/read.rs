                                                                             
                                  

use crate::confine::{ConfinedPath, RelPath, Root, escape_control_chars};

use super::{ToolBounds, ToolError};

pub(crate) fn read_file(
    root: &Root,
    bounds: &ToolBounds,
    path: &RelPath,
    range: Option<(u64, u64)>,
) -> Result<String, ToolError> {
    let cpath = ConfinedPath::within(root, path.as_path())?;
                                                                              
                                                                             
                                                                      
    let mut bytes = root.read_confined(&cpath, bounds.max_file_bytes.saturating_add(1))?;
    let truncated = bytes.len() as u64 > bounds.max_file_bytes;
    if truncated {
        bytes.truncate(usize::try_from(bounds.max_file_bytes).unwrap_or(usize::MAX));
    }
    let text = String::from_utf8_lossy(&bytes);

    let (start, end) = range.unwrap_or((1, u64::MAX));
    let mut out = String::new();
    let mut shown = 0u64;
    for (idx, line) in text.lines().enumerate() {
        let n = idx as u64 + 1;
        if n < start {
            continue;
        }
        if n > end {
            break;
        }
        out.push_str(&format!("{n:>5}: {line}\n"));
        shown += 1;
    }
    if shown == 0 {
        out.push_str("(no lines in the requested range)\n");
    }
    if truncated {
                                                                              
                                                                          
        out.push_str(&format!(
            "[truncated: only the first {} bytes of {} were read]\n",
            bounds.max_file_bytes,
            escape_control_chars(path.scope_str())
        ));
    }
    Ok(out)
}
