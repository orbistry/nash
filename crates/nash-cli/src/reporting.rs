use std::path::{Path, PathBuf};

/// Show source files relative to the loaded project, independently of shell CWD.
/// OSC 8 links retain absolute, URL-escaped file targets behind those labels.
pub(crate) fn source_name(root: &Path, links: bool, name: &str) -> String {
    let path = Path::new(name);
    let Ok(uri) = url::Url::from_file_path(path) else {
        return name.to_owned();
    };
    let relative = root
        .ancestors()
        .enumerate()
        .find_map(|(parents, ancestor)| {
            path.strip_prefix(ancestor).ok().map(|tail| {
                std::iter::repeat_n("..", parents)
                    .collect::<PathBuf>()
                    .join(tail)
            })
        });
    // Different Windows volumes have no relative path between them.
    let label = relative.as_deref().unwrap_or(path).to_string_lossy();
    if links {
        format!("\x1b]8;;{uri}\x1b\\{label}\x1b]8;;\x1b\\")
    } else {
        label.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_labels_keep_absolute_escaped_link_targets() {
        let root = std::env::temp_dir().join("nash path # π");
        let file = root.join("member/src/Main file # π.nash");
        let label = Path::new("member/src/Main file # π.nash")
            .display()
            .to_string();
        let target = url::Url::from_file_path(&file).unwrap();
        let name = file.to_str().unwrap();
        assert_eq!(source_name(&root, false, name), label);
        assert_eq!(
            source_name(&root, true, name),
            format!("\x1b]8;;{target}\x1b\\{label}\x1b]8;;\x1b\\")
        );
        assert!(target.as_str().contains("%20"));
        assert!(target.as_str().contains("%23"));
    }

    #[test]
    fn sibling_dependencies_are_relative_to_the_project_too() {
        let base = std::env::temp_dir();
        let root = base.join("application");
        let file = base.join("dependency/src/Main.nash");
        assert_eq!(
            source_name(&root, false, file.to_str().unwrap()),
            Path::new("../dependency/src/Main.nash")
                .display()
                .to_string()
        );
    }

    #[test]
    fn non_file_source_names_are_preserved() {
        assert_eq!(
            source_name(Path::new("/project"), true, "Builtin"),
            "Builtin"
        );
    }
}
