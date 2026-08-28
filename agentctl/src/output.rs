use colored::Colorize;
use similar::{ChangeTag, TextDiff};
use std::path::Path;

pub fn print_file_diff(path: &Path, old: &str, new: &str) {
    if old == new {
        println!("{} {} {}", "[unchanged]".dimmed(), path.display(), "(no changes)".dimmed());
        return;
    }

    let diff = TextDiff::from_lines(old, new);
    println!("{} {}", "diff ".cyan().bold(), format!("-- {}", path.display()).cyan());
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-".red(),
            ChangeTag::Insert => "+".green(),
            ChangeTag::Equal => " ".normal(),
        };
        let line = match change.tag() {
            ChangeTag::Delete => change.to_string().red(),
            ChangeTag::Insert => change.to_string().green(),
            ChangeTag::Equal => change.to_string().normal(),
        };
        print!("{sign}{line}");
    }
}

pub fn print_tree_changes(source_dir: &Path, target_dir: &Path) {
    match tree_differs(source_dir, target_dir) {
        tree_differs::TreeStatus::Same => {
            println!(
                "{} {} {}",
                "[unchanged]".dimmed(),
                target_dir.display(),
                "(no changes)".dimmed()
            );
        }
        tree_differs::TreeStatus::Missing(_) | tree_differs::TreeStatus::Changed => {
            println!(
                "{}",
                format!("[tree] {} -> {}", source_dir.display(), target_dir.display())
                    .cyan()
                    .bold()
            );
        }
    }
}

pub fn tree_differs(src: &Path, dst: &Path) -> tree_differs::TreeStatus {
    if !dst.exists() {
        return tree_differs::TreeStatus::Missing("target directory does not exist".into());
    }
    match (src.is_dir(), dst.is_dir()) {
        (true, true) => {
            let src_entries =
                std::fs::read_dir(src).map(|it| it.filter_map(|e| e.ok()).collect::<Vec<_>>());
            let dst_entries =
                std::fs::read_dir(dst).map(|it| it.filter_map(|e| e.ok()).collect::<Vec<_>>());
            match (src_entries, dst_entries) {
                (Ok(s), Ok(d)) => {
                    for entry in &s {
                        let name = entry.file_name();
                        let s_path = src.join(&name);
                        let d_path = dst.join(&name);
                        if !d_path.exists() {
                            return tree_differs::TreeStatus::Missing(
                                format!("missing file {}", d_path.display()),
                            );
                        }
                        if s_path.is_dir() != d_path.is_dir() {
                            return tree_differs::TreeStatus::Changed;
                        }
                        if let Some(diff) = tree_differs(&s_path, &d_path).into_option() {
                            return diff;
                        }
                    }
                    // Check for extra files in dst that are not in src (removed skills).
                    let src_names: Vec<_> = s.iter().map(|e| e.file_name()).collect();
                    for entry in &d {
                        if !src_names.contains(&entry.file_name()) {
                            return tree_differs::TreeStatus::Changed;
                        }
                    }
                    tree_differs::TreeStatus::Same
                }
                _ => tree_differs::TreeStatus::Changed,
            }
        }
        (true, false) | (false, true) => tree_differs::TreeStatus::Changed,
        (false, false) => {
            if !src.exists() {
                return tree_differs::TreeStatus::Changed;
            }
            if std::fs::read(src).ok() != std::fs::read(dst).ok() {
                tree_differs::TreeStatus::Changed
            } else {
                tree_differs::TreeStatus::Same
            }
        }
    }
}

pub mod tree_differs {
    #[derive(Debug, Clone, PartialEq)]
    pub enum TreeStatus {
        Same,
        Missing(String),
        Changed,
    }

    impl TreeStatus {
        pub fn into_option(self) -> Option<Self> {
            match self {
                TreeStatus::Same => None,
                other => Some(other),
            }
        }
    }
}

#[allow(dead_code)]
pub fn diff_with_lines(old: &str, new: &str) -> bool {
    old != new
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::tree_differs::TreeStatus;

    #[test]
    fn diff_detects_changes() {
        assert!(diff_with_lines("a\n", "b\n"));
        assert!(!diff_with_lines("a\n", "a\n"));
    }

    #[test]
    fn tree_same_when_identical() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("f.txt"), "x").unwrap();
        std::fs::write(b.join("f.txt"), "x").unwrap();
        assert_eq!(tree_differs(&a, &b), TreeStatus::Same);
    }

    #[test]
    fn tree_changed_when_content_differs() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("f.txt"), "x").unwrap();
        std::fs::write(b.join("f.txt"), "y").unwrap();
        assert_eq!(tree_differs(&a, &b), TreeStatus::Changed);
    }

    #[test]
    fn tree_changed_when_file_added() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("f.txt"), "x").unwrap();
        assert_eq!(tree_differs(&a, &b), TreeStatus::Missing("missing file ".to_owned() + &b.join("f.txt").display().to_string()));
    }

    #[test]
    fn tree_changed_when_stray_file_in_dst() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("unmanaged.txt"), "x").unwrap();
        assert_eq!(tree_differs(&a, &b), TreeStatus::Changed);
    }
}
