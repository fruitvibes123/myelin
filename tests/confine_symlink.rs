                                                                      
                                                                

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use myelin::confine::{ConfineError, ConfinedPath, Root};

const OUTSIDE: &[u8] = b"OUTSIDE-SECRET";
const INSIDE: &[u8] = b"inside-ok";

                                                                       
                                              
fn setup() -> (tempfile::TempDir, Root) {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir(tmp.path().join("outside")).unwrap();
    fs::write(tmp.path().join("outside/secret.txt"), OUTSIDE).unwrap();
    fs::create_dir(tmp.path().join("root")).unwrap();
    let root = Root::open(&tmp.path().join("root")).unwrap();
    (tmp, root)
}

#[test]
fn final_component_symlink_is_not_followed() {
    let (tmp, root) = setup();
    symlink(
        tmp.path().join("outside/secret.txt"),
        tmp.path().join("root/link.txt"),
    )
    .unwrap();

                                                                                  
    assert!(
        matches!(
            ConfinedPath::within(&root, Path::new("link.txt")),
            Err(ConfineError::FinalComponentSymlink)
        ),
        "within followed or accepted a final-component symlink"
    );

                                                                                 
                                                                                 
                                                                                
    fs::write(tmp.path().join("root/real.txt"), INSIDE).unwrap();
    let p = ConfinedPath::within(&root, Path::new("real.txt")).unwrap();
    fs::remove_file(tmp.path().join("root/real.txt")).unwrap();
    symlink(
        tmp.path().join("outside/secret.txt"),
        tmp.path().join("root/real.txt"),
    )
    .unwrap();
    let res = root.read_confined(&p, 1 << 16);
    assert!(matches!(res, Err(ConfineError::Escape)), "got {res:?}");
}

#[test]
fn within_refuses_a_final_component_symlink_eagerly() {
                                                                               
                                                                             
                                                                          
                                                                                  
                                                                 
    let (tmp, root) = setup();
    symlink(
        tmp.path().join("outside/secret.txt"),
        tmp.path().join("root/escape"),
    )
    .unwrap();
    symlink(
        tmp.path().join("outside"),
        tmp.path().join("root/escape_dir"),
    )
    .unwrap();
                                                                                  
                                                                             
    fs::write(tmp.path().join("root/real.txt"), INSIDE).unwrap();
    symlink("real.txt", tmp.path().join("root/inroot_link")).unwrap();

    for name in ["escape", "escape_dir", "inroot_link"] {
        assert!(
            matches!(
                ConfinedPath::within(&root, Path::new(name)),
                Err(ConfineError::FinalComponentSymlink)
            ),
            "final-component symlink {name} was accepted or mislabelled"
        );
    }

                                               
    assert!(ConfinedPath::within(&root, Path::new("real.txt")).is_ok());
}

#[test]
fn intermediate_symlink_dir_is_not_followed() {
    let (tmp, root) = setup();
    symlink(tmp.path().join("outside"), tmp.path().join("root/sub")).unwrap();

    let res = ConfinedPath::within(&root, Path::new("sub/secret.txt"));
    assert!(matches!(res, Err(ConfineError::Escape)), "got {res:?}");
}

#[test]
fn even_inside_pointing_symlink_is_refused() {
    let (tmp, root) = setup();
    fs::write(tmp.path().join("root/real.txt"), INSIDE).unwrap();
    symlink("real.txt", tmp.path().join("root/alias.txt")).unwrap();

                                                                           
    assert!(
        matches!(
            ConfinedPath::within(&root, Path::new("alias.txt")),
            Err(ConfineError::FinalComponentSymlink)
        ),
        "within accepted an in-root-pointing final-component symlink"
    );

                                                                              
                                                                                 
                             
    let p = ConfinedPath::within(&root, Path::new("real.txt")).unwrap();
    fs::write(tmp.path().join("root/real2.txt"), INSIDE).unwrap();
    fs::remove_file(tmp.path().join("root/real.txt")).unwrap();
    symlink("real2.txt", tmp.path().join("root/real.txt")).unwrap();
    let res = root.read_confined(&p, 1 << 16);
    assert!(matches!(res, Err(ConfineError::Escape)), "got {res:?}");
}

                                                                            
                                                                   
                                                                           
                                                                       
                              
#[test]
fn concurrent_dir_symlink_swap_never_escapes() {
    let (tmp, root) = setup();
    fs::create_dir(tmp.path().join("root/swap")).unwrap();
    fs::write(tmp.path().join("root/swap/secret.txt"), INSIDE).unwrap();
    symlink(tmp.path().join("outside"), tmp.path().join("root/alt")).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let flipper = {
        let stop = Arc::clone(&stop);
        let dir = tmp.path().to_path_buf();
        std::thread::spawn(move || {
            let mut flips = 0u64;
            while !stop.load(Ordering::Relaxed) {
                rustix::fs::renameat_with(
                    rustix::fs::CWD,
                    dir.join("root/swap"),
                    rustix::fs::CWD,
                    dir.join("root/alt"),
                    rustix::fs::RenameFlags::EXCHANGE,
                )
                .unwrap();
                flips += 1;
            }
            flips
        })
    };

    let mut inside_reads = 0u64;
    for _ in 0..5_000 {
        let Ok(p) = ConfinedPath::within(&root, Path::new("swap/secret.txt")) else {
            continue;                                                                    
        };
                                                                             
        if let Ok(bytes) = root.read_confined(&p, 1 << 16) {
            assert_eq!(bytes, INSIDE, "confinement leaked the outside file");
            inside_reads += 1;
        }
    }

    stop.store(true, Ordering::Relaxed);
    let flips = flipper.join().unwrap();
                                                                            
    assert!(flips > 0, "flipper never ran");
    assert!(inside_reads > 0, "reader never saw the real directory");
}
