//! One directory walk for sidecar and WSDL discovery (`inbound`, `xmllane`,
//! `doctor`).
//!
//! * A directory is entered once. The walk tracks directories by canonical
//!   path, so a symlink cycle (`loop -> .`) adds nothing.
//! * Symlinks to files and to directories still work. Kubernetes volume mounts
//!   use them.
//! * A directory whose name starts with `.` is skipped. This covers the
//!   `..data` and `..<timestamp>` directories of a Kubernetes ConfigMap mount,
//!   which hold the same files a second time.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// All files under `root`, sorted. The result has no path twice.
pub fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    if let Ok(c) = root.canonicalize() {
        seen.insert(c);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for p in rd.flatten().map(|e| e.path()) {
            if p.is_dir() {
                let hidden = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'));
                if !hidden && p.canonicalize().is_ok_and(|c| seen.insert(c)) {
                    stack.push(p);
                }
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    fn names(root: &Path) -> Vec<String> {
        files(root)
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().display().to_string())
            .collect()
    }

    #[test]
    fn one_cycle_adds_nothing() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("g/a.yaml"));
        symlink(".", d.path().join("g/loop")).unwrap();
        assert_eq!(names(d.path()), ["g/a.yaml"]);
    }

    #[test]
    fn many_cycles_finish_at_once() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("g/a.yaml"));
        for i in 0..6 {
            symlink(".", d.path().join(format!("g/loop{i}"))).unwrap();
            symlink("..", d.path().join(format!("g/up{i}"))).unwrap();
        }
        assert_eq!(names(d.path()), ["g/a.yaml"]);
    }

    #[test]
    fn a_cycle_through_two_directories_adds_nothing() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("a/f.yaml"));
        std::fs::create_dir_all(d.path().join("b")).unwrap();
        symlink("../b", d.path().join("a/to_b")).unwrap();
        symlink("../a", d.path().join("b/to_a")).unwrap();
        assert_eq!(names(d.path()), ["a/f.yaml"]);
    }

    #[test]
    fn symlinked_files_are_listed() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("g/real.yaml"));
        symlink("real.yaml", d.path().join("g/link.yaml")).unwrap();
        assert_eq!(names(d.path()), ["g/link.yaml", "g/real.yaml"]);
    }

    /// Kubernetes ConfigMap layout: `g -> ..data/g`, `..data -> ..ts`, real
    /// files only in `..ts`. The group appears once, under its public name.
    #[test]
    fn configmap_mount_layout_lists_each_file_once() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("..2026_11_12/g/x.xml.yaml"));
        symlink("..2026_11_12", d.path().join("..data")).unwrap();
        symlink("..data/g", d.path().join("g")).unwrap();
        assert_eq!(names(d.path()), ["g/x.xml.yaml"]);
    }
}
