                                                                            
                                                              

use myelin::confine::Root;
use myelin::tools::{ToolBounds, ToolCall, Toolbox};

const MARKER: &str = "[stopped early:";

                                                                                 
                                                                               
                                                                                 
                                                                                  
const SLOW: &str = "(?:a|b){2000}c";

                                                                                 
                                                                                    
                                                                                   
                                                                                   
                                                        
fn slow_corpus() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let line = "a".repeat(50_000);
    let mut body = String::new();
    for _ in 0..5 {
        body.push_str(&line);
        body.push('\n');
    }
    for f in 0..8 {
        std::fs::write(tmp.path().join(format!("f{f}.txt")), &body).unwrap();
    }
    tmp
}

fn plain_corpus() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for f in 0..8 {
        std::fs::write(
            tmp.path().join(format!("f{f}.txt")),
            "the quick brown fox\njumps over\nthe lazy dog\n",
        )
        .unwrap();
    }
    tmp
}

                                                                               
                                                                       
                                                                            
                                                                     
fn dir_only_corpus(top: usize, sub: usize) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for i in 0..top {
        let d = tmp.path().join(format!("d{i:04}"));
        std::fs::create_dir(&d).unwrap();
        for j in 0..sub {
            std::fs::create_dir(d.join(format!("s{j:04}"))).unwrap();
        }
    }
    tmp
}

fn run_grep(dir: &std::path::Path, pattern: &str, grep_deadline_ms: u64) -> String {
    let root = Root::open(dir).unwrap();
    let tb = Toolbox {
        root: &root,
        bounds: ToolBounds {
            grep_deadline_ms,
            ..ToolBounds::default()
        },
        scope: None,
        git_enabled: false,
        search: None,
        excluded_dirs: &[],
    };
    tb.dispatch(&ToolCall::Grep {
        pattern: pattern.into(),
        path: None,
    })
    .expect("grep dispatch")
}

#[test]
fn slow_pattern_hits_the_deadline() {
    let tmp = slow_corpus();
                                                                              
                                                                                  
    let start = std::time::Instant::now();
    let out = run_grep(tmp.path(), SLOW, 20);
    let elapsed = start.elapsed();
    assert!(
        out.contains(MARKER),
        "grep did not stop on the deadline: {out:?}"
    );
                                                                                  
                                                                                    
                                                                                   
                                                                          
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "grep took {elapsed:?} at a 20 ms deadline — the per-line deadline is not bounding the scan"
    );
}

#[test]
fn walk_budget_deadline_bounds_a_directory_only_tree() {
                                                                                
                                                                                 
                                                                                
    let tmp = dir_only_corpus(200, 100);
    let start = std::time::Instant::now();
    let out = run_grep(tmp.path(), "CANARY", 10);
    let elapsed = start.elapsed();
    assert!(
        out.contains(MARKER),
        "the walk-budget deadline did not fire on a directory-only tree: {out:?}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the walk-budget deadline did not bound the walk ({elapsed:?})"
    );
}

#[test]
fn plain_pattern_under_default_deadline_completes() {
    let tmp = plain_corpus();
    let out = run_grep(tmp.path(), "fox", 30_000);
    assert!(
        !out.contains(MARKER),
        "plain grep should not be capped: {out:?}"
    );
    assert!(out.contains("fox"), "expected the match: {out:?}");
}
