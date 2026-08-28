                     
   
                                                                             
                                                                            
                                                                                
                                                                                
                                                                          
                                                                                  
                                                                               
                                                                             
                                    
   
                                                                            
                                                                            
                                                      
                                                                              
                                                                           
                                                                                
   
                                                                            
                                                                    
                                                                        

use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use rustix::fs::{Mode, OFlags, ResolveFlags};

                                                                             
#[derive(Debug, thiserror::Error)]
pub enum ConfineError {
    #[error("empty path")]
    Empty,
    #[error("absolute paths are not allowed")]
    Absolute,
    #[error("`..` (parent traversal) is not allowed")]
    ParentTraversal,
    #[error("NUL byte in path")]
    Nul,
                                                                            
    #[error("path escapes the confined root")]
    Escape,
                                                                        
                                                                                
                                                                               
    #[error("final component is a symlink, not followed")]
    FinalComponentSymlink,
                                                                             
    #[error("path crosses a filesystem boundary (device pin)")]
    CrossDevice,
    #[error("path not found within root")]
    NotFound,
                                                                             
                                                                       
    #[error("root directory does not exist")]
    RootNotFound,
    #[error("permission denied within root")]
    Denied,
                                                                                     
    #[error("kernel lacks openat2(2); refusing unsafe fallback")]
    Openat2Unavailable,
    #[error("not a regular file")]
    NotRegularFile,
    #[error("not a directory")]
    NotDirectory,
                                                                                  
                                        
    #[error("secrets anchor path must be absolute")]
    AnchorRelative,
                                               
    #[error("secrets anchor not found")]
    AnchorNotFound,
                                                                                 
                                                                                
                   
    #[error("secrets anchor path contains a symlink (refused, not followed)")]
    AnchorSymlink,
    #[error("i/o error during confined open: {0}")]
    Io(rustix::io::Errno),
    #[error("read failed: {0}")]
    ReadIo(#[from] std::io::Error),
}

                                                   
   
                                                                                
                                                                             
                                                                                      
                                                                                
                                                                         
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

impl Entry {
                                                                           
                                                                                  
    pub fn display_name(&self) -> String {
        escape_control_chars(&self.name)
    }
}

                                                                              
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

                                                                              
                                                                     
#[derive(Debug)]
pub struct Root {
    fd: OwnedFd,
    dev: u64,
    label: PathBuf,
}

impl Root {
                                                                           
                                                                         
                                                    
    pub fn open(path: &Path) -> Result<Root, ConfineError> {
                                                                                 
        let fd = rustix::fs::open(
            path,
            OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(map_open_errno)?;
        let st = rustix::fs::fstat(&fd).map_err(ConfineError::Io)?;
        if rustix::fs::FileType::from_raw_mode(st.st_mode) != rustix::fs::FileType::Directory {
            return Err(ConfineError::NotDirectory);
        }
        Ok(Root {
            fd,
            dev: st.st_dev as u64,
            label: path.to_path_buf(),
        })
    }

                                                                             
                                                                 
                                                                    
    pub fn device(&self) -> u64 {
        self.dev
    }

                                                                   
    pub fn label(&self) -> &Path {
        &self.label
    }

                                                                            
                                                                                
                                                                                
                                                                              
                                                           
    pub fn open_confined(
        &self,
        path: &ConfinedPath,
        oflags: OFlags,
    ) -> Result<OwnedFd, ConfineError> {
        self.resolve(&path.rel, oflags)
    }

                                                                              
                                                                                
                                    
    pub fn read_confined(
        &self,
        path: &ConfinedPath,
        max_bytes: u64,
    ) -> Result<Vec<u8>, ConfineError> {
        use std::io::Read;
                                                                                  
                                                                                  
        let fd = self.open_confined(path, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK)?;
        let st = rustix::fs::fstat(&fd).map_err(ConfineError::Io)?;
        if rustix::fs::FileType::from_raw_mode(st.st_mode) != rustix::fs::FileType::RegularFile {
            return Err(ConfineError::NotRegularFile);
        }
        let file = std::fs::File::from(fd);
        let mut buf = Vec::new();
        file.take(max_bytes).read_to_end(&mut buf)?;
        Ok(buf)
    }

                                                                             
                                                                        
                                                                
    pub fn list_confined(
        &self,
        path: &ConfinedPath,
        max_entries: usize,
    ) -> Result<Vec<Entry>, ConfineError> {
        let fd = self.open_confined(path, OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW)?;
        let dir = rustix::fs::Dir::new(fd).map_err(ConfineError::Io)?;
        let mut out = Vec::new();
        for entry in dir {
            if out.len() >= max_entries {
                break;
            }
            let entry = entry.map_err(ConfineError::Io)?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            let kind = match entry.file_type() {
                rustix::fs::FileType::RegularFile => EntryKind::File,
                rustix::fs::FileType::Directory => EntryKind::Dir,
                rustix::fs::FileType::Symlink => EntryKind::Symlink,
                _ => EntryKind::Other,
            };
            out.push(Entry {
                name: String::from_utf8_lossy(bytes).into_owned(),
                kind,
            });
        }
        Ok(out)
    }

    fn resolve(&self, rel: &Path, oflags: OFlags) -> Result<OwnedFd, ConfineError> {
        let resolve = ResolveFlags::BENEATH
            | ResolveFlags::NO_SYMLINKS
            | ResolveFlags::NO_XDEV
            | ResolveFlags::NO_MAGICLINKS;
        let fd = rustix::fs::openat2(
            &self.fd,
            rel,
            oflags | OFlags::CLOEXEC,
            Mode::empty(),
            resolve,
        )
        .map_err(map_resolve_errno)?;

                                                                             
                                                                            
                                                        
        let st = rustix::fs::fstat(&fd).map_err(ConfineError::Io)?;
        if st.st_dev as u64 != self.dev {
            return Err(ConfineError::CrossDevice);
        }
        Ok(fd)
    }
}

                                                                              
                                                                          
                                                                              
                                                                           
                                                                         
pub fn escape_control_chars(s: &str) -> String {
    if !s.chars().any(char::is_control) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() {
            out.push_str(&format!("<0x{:02X}>", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

                                                                                 
                                                                                  
                                                                            
                                                                                
                                                                       
   
                                                                              
                                                                          
                                                                      
                                                          
                                                                             
                                                 
   
                                                                                
                                                                                
                                                                                  
                                                                                           
pub fn anchor_device(abs: &Path) -> Result<u64, ConfineError> {
    if !abs.is_absolute() {
        return Err(ConfineError::AnchorRelative);
    }
    let fd = rustix::fs::openat2(
        rustix::fs::CWD,
        abs,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS,
    )
    .map_err(map_anchor_errno)?;
    let st = rustix::fs::fstat(&fd).map_err(ConfineError::Io)?;
    Ok(st.st_dev as u64)
}

                                                                                
#[derive(Debug, thiserror::Error)]
#[error("`{a}` and `{b}` share filesystem device {dev}; they must be on separate filesystems")]
pub struct DeviceCollision {
    pub a: String,
    pub b: String,
    pub dev: u64,
}

                                                                                
                                                                                
                                                               
   
                                                                                  
                                                                                
                                                                 
                                                                          
                  
pub fn devices_disjoint(labelled: &[(&str, u64)]) -> Result<(), DeviceCollision> {
    for (i, &(a, da)) in labelled.iter().enumerate() {
        for &(b, db) in &labelled[i + 1..] {
            if da == db {
                return Err(DeviceCollision {
                    a: a.to_string(),
                    b: b.to_string(),
                    dev: da,
                });
            }
        }
    }
    Ok(())
}

                                                                                
   
                                                                             
                                                                                
                                                                                
                                                                       
#[derive(Debug, Clone)]
pub struct ConfinedPath {
    rel: PathBuf,
}

impl ConfinedPath {
                                                                             
                                                                              
                                                                                 
                                                                               
                                                                               
                                                                              
                                                                                 
                                                                                 
                                                    
    pub fn within(root: &Root, raw: &Path) -> Result<ConfinedPath, ConfineError> {
        let rel = validate_relative(raw)?;
                                                                                   
                                                                                    
                                                                                    
                                                                                   
                                                                              
        let probe = root.resolve(&rel, OFlags::PATH | OFlags::NOFOLLOW)?;
        let st = rustix::fs::fstat(&probe).map_err(ConfineError::Io)?;
        if rustix::fs::FileType::from_raw_mode(st.st_mode) == rustix::fs::FileType::Symlink {
            return Err(ConfineError::FinalComponentSymlink);
        }
        Ok(ConfinedPath { rel })
    }

                                                                  
    pub fn rel(&self) -> &Path {
        &self.rel
    }
}

                                                                               
                                                                              
                                                                              
                                                                                
                     
fn normalized_components(raw: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in raw.components() {
        match comp {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

                                                                             
                                                                  
                                                                                 
                                                                                
                                                       
pub(crate) fn scope_rel_string(raw: &str) -> String {
    normalized_components(Path::new(raw))
        .to_string_lossy()
        .into_owned()
}

                                                                       
                                                                               
                                                                               
                                        
pub(crate) struct RelPath(String);

impl RelPath {
    pub(crate) fn normalize(raw: &str) -> RelPath {
        RelPath(scope_rel_string(raw))
    }

                                                               
    pub(crate) fn scope_str(&self) -> &str {
        &self.0
    }

                                                                          
    pub(crate) fn as_path(&self) -> &Path {
        if self.0.is_empty() {
            Path::new(".")
        } else {
            Path::new(&self.0)
        }
    }
}

                                                                                
                                                                               
                                                              
fn validate_relative(raw: &Path) -> Result<PathBuf, ConfineError> {
    if raw.as_os_str().is_empty() {
        return Err(ConfineError::Empty);
    }
    let mut out = PathBuf::new();
    for comp in normalized_components(raw).components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => return Err(ConfineError::Absolute),
            Component::ParentDir => return Err(ConfineError::ParentTraversal),
            Component::CurDir => {}
            Component::Normal(c) => {
                if c.as_bytes().contains(&0) {
                    return Err(ConfineError::Nul);
                }
                out.push(c);
            }
        }
    }
    if out.as_os_str().is_empty() {
                                                                      
        out.push(".");
    }
    Ok(out)
}

                                                                             
                                                         
fn map_open_errno(e: rustix::io::Errno) -> ConfineError {
    match e {
        rustix::io::Errno::NOENT => ConfineError::RootNotFound,
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => ConfineError::Denied,
        rustix::io::Errno::NOTDIR => ConfineError::NotDirectory,
        other => ConfineError::Io(other),
    }
}

fn map_resolve_errno(e: rustix::io::Errno) -> ConfineError {
    match e {
                                                                               
                                                                                   
                                                                                  
        rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL => ConfineError::Openat2Unavailable,
                                                                                 
                                                                                   
                                                              
        rustix::io::Errno::LOOP | rustix::io::Errno::XDEV => ConfineError::Escape,
                                                                                 
                                                                               
                                                                   
        rustix::io::Errno::AGAIN => ConfineError::Escape,
        rustix::io::Errno::NOENT => ConfineError::NotFound,
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => ConfineError::Denied,
        rustix::io::Errno::NOTDIR => ConfineError::NotDirectory,
        other => ConfineError::Io(other),
    }
}

                                                                              
                                                                                      
                                                                             
                                                                                  
                              
fn map_anchor_errno(e: rustix::io::Errno) -> ConfineError {
    match e {
                                                                               
                                                                  
        rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL => ConfineError::Openat2Unavailable,
                                                                                
        rustix::io::Errno::LOOP => ConfineError::AnchorSymlink,
        rustix::io::Errno::NOENT => ConfineError::AnchorNotFound,
                                                                                
        rustix::io::Errno::NOTDIR => ConfineError::NotDirectory,
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => ConfineError::Denied,
        other => ConfineError::Io(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_parent_traversal() {
        assert!(matches!(
            validate_relative(Path::new("../../etc/passwd")),
            Err(ConfineError::ParentTraversal)
        ));
        assert!(matches!(
            validate_relative(Path::new("a/../../etc")),
            Err(ConfineError::ParentTraversal)
        ));
    }

    #[test]
    fn rejects_absolute() {
        assert!(matches!(
            validate_relative(Path::new("/etc/passwd")),
            Err(ConfineError::Absolute)
        ));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(
            validate_relative(Path::new("")),
            Err(ConfineError::Empty)
        ));
    }

    #[test]
    fn accepts_clean_relative_and_drops_dot() {
        let p = validate_relative(Path::new("./src/./main.rs")).expect("valid");
        assert_eq!(p, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn bare_dot_names_root() {
        let p = validate_relative(Path::new(".")).expect("valid");
        assert_eq!(p, PathBuf::from("."));
    }

                                                     

    fn mk_dir(base: &Path, tag: &str) -> PathBuf {
        let p = base.join(format!("myelin-g2-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("create test dir");
        p
    }

    fn dev_of(p: &Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(p).expect("stat").dev()
    }

    #[test]
    fn anchor_device_rejects_relative() {
        assert!(matches!(
            anchor_device(Path::new("relative/secrets")),
            Err(ConfineError::AnchorRelative)
        ));
        assert!(matches!(
            anchor_device(Path::new("secrets")),
            Err(ConfineError::AnchorRelative)
        ));
    }

    #[test]
    fn anchor_device_missing_and_not_a_directory() {
        let base = mk_dir(&std::env::temp_dir(), "missing-notdir");
                                    
        assert!(matches!(
            anchor_device(&base.join("does-not-exist")),
            Err(ConfineError::AnchorNotFound)
        ));
                                                                  
        let file = base.join("a-file");
        std::fs::write(&file, b"x").expect("write");
        assert!(matches!(
            anchor_device(&file),
            Err(ConfineError::NotDirectory)
        ));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn devices_disjoint_oracle() {
                              
        assert!(
            devices_disjoint(&[("root", 1), ("secrets", 2), ("scratch", 3), ("log", 4)]).is_ok()
        );
                                                                     
        assert!(devices_disjoint(&[]).is_ok());
        assert!(devices_disjoint(&[("only", 7)]).is_ok());
                                                                  
        let err = devices_disjoint(&[("root", 1), ("secrets", 2), ("scratch", 2), ("log", 4)])
            .expect_err("collision");
        assert_eq!(
            (err.a.as_str(), err.b.as_str(), err.dev),
            ("secrets", "scratch", 2)
        );
                                            
        let err2 = devices_disjoint(&[("root", 5), ("secrets", 5)]).expect_err("collision");
        assert_eq!(
            (err2.a.as_str(), err2.b.as_str(), err2.dev),
            ("root", "secrets", 5)
        );
    }

    #[test]
    fn cross_fs_resolver_redirect_and_composition() {
        use std::os::unix::fs::symlink;
                                                                                   
                                                                                    
                                                                                  
                                                                                     
                                                                               
                                                                                
                                                                                  
                                                          
        let mut candidates: Vec<PathBuf> = vec![
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target"),
            std::env::temp_dir(),
            PathBuf::from("/dev/shm"),
            PathBuf::from("/var/tmp"),
        ];
        if let Some(d) = std::env::var_os("MYELIN_TEST_XFS_DIR") {
            candidates.push(PathBuf::from(d));
        }
                                                                                
        let made: Vec<(PathBuf, u64)> = candidates
            .iter()
            .enumerate()
            .filter(|(_, base)| base.is_dir())
            .filter_map(|(i, base)| {
                let p = base.join(format!("myelin-g2-{}-xfs{}", std::process::id(), i));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p)
                    .ok()
                    .map(|()| (p.clone(), dev_of(&p)))
            })
            .collect();
        let cleanup = |made: &[(PathBuf, u64)]| {
            for (p, _) in made {
                let _ = std::fs::remove_dir_all(p);
            }
        };
                                              
        let mut pair = None;
        'find: for i in 0..made.len() {
            for j in (i + 1)..made.len() {
                if made[i].1 != made[j].1 {
                    pair = Some((made[i].0.clone(), made[j].0.clone()));
                    break 'find;
                }
            }
        }
        let (root_dir, secrets_dir) = match pair {
            Some(p) => p,
            None => {
                cleanup(&made);
                panic!(
                    "none of {candidates:?} are on distinct filesystems on this box; set \
                     MYELIN_TEST_XFS_DIR to a dir on a second filesystem. The cross-fs \
                     differential proves nothing without two devices, so this is a hard \
                     failure, never a silent skip"
                );
            }
        };

                                                                       
        let root_dev = anchor_device(&root_dir).expect("root dev");
        let secrets_dev = anchor_device(&secrets_dir).expect("secrets dev");
        assert_eq!(root_dev, dev_of(&root_dir));
        assert_eq!(secrets_dev, dev_of(&secrets_dir));
        assert_ne!(root_dev, secrets_dev);

                                                                               
                                                                                
                               
        let link = root_dir.join("redirect");
        symlink(&secrets_dir, &link).expect("symlink");
        assert!(matches!(
            anchor_device(&link),
            Err(ConfineError::AnchorSymlink)
        ));
                                                                             
                                                                                
                                                               
        assert_eq!(dev_of(&link), secrets_dev);                                    
        assert_ne!(dev_of(&link), root_dev);                                 

                                                                  
                                                                   
        let root = Root::open(&root_dir).expect("open root");
        assert_eq!(root.device(), root_dev);
        assert!(devices_disjoint(&[("root", root.device()), ("secrets", secrets_dev)]).is_ok());
                                                                               
        assert!(devices_disjoint(&[("root", root_dev), ("secrets", root_dev)]).is_err());

        cleanup(&made);
    }
}
