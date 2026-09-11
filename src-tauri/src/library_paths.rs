use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const PAPERS_DIR: &str = "Papers";
pub const TEXTBOOKS_DIR: &str = "Textbooks";
pub const EXPORT_DIR: &str = "export";
pub const INTERNAL_DIR: &str = ".read-desktop";
pub const CHARACTERS_DIR: &str = "Characters";
pub const LONG_PDF_PAGE_LIMIT: i64 = 80;

pub const PAPERS_ROOT_COLLECTION_ID: &str = "collection-root";
pub const TEXTBOOKS_ROOT_COLLECTION_ID: &str = "collection-textbooks-root";

const UNMANAGED_LIST_CAP: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    #[default]
    Paper,
    Textbook,
}

impl DocumentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Textbook => "textbook",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Paper => PAPERS_DIR,
            Self::Textbook => TEXTBOOKS_DIR,
        }
    }

    pub fn from_dir_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case(PAPERS_DIR) {
            Some(Self::Paper)
        } else if name.eq_ignore_ascii_case(TEXTBOOKS_DIR) {
            Some(Self::Textbook)
        } else {
            None
        }
    }
}

pub fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/").trim().trim_matches('/').to_string()
}

pub fn first_segment(relative: &str) -> Option<&str> {
    let normalized = relative.trim().trim_start_matches('/');
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.split(['/', '\\']).next().unwrap_or(normalized))
}

pub fn document_kind_from_relative(relative: &str) -> Option<DocumentKind> {
    first_segment(relative).and_then(DocumentKind::from_dir_name)
}

pub fn is_reserved_workspace_entry(name: &str) -> bool {
    name.eq_ignore_ascii_case(PAPERS_DIR)
        || name.eq_ignore_ascii_case(TEXTBOOKS_DIR)
        || name.eq_ignore_ascii_case(EXPORT_DIR)
        || name.eq_ignore_ascii_case(INTERNAL_DIR)
        || name.eq_ignore_ascii_case(CHARACTERS_DIR)
}

pub fn looks_like_library_relative(relative: &str) -> bool {
    document_kind_from_relative(relative).is_some()
}

/// Prefix a pre-schema-5 path with `Papers/` unless it already has a library
/// root plus at least one extra segment (`Papers/Inbox`, `Textbooks/ch.pdf`).
/// A bare `Papers` / `Textbooks` name is treated as a nested folder, not a root.
pub fn prefix_legacy_relative(relative: &str) -> String {
    let normalized = normalize_slashes(relative);
    if normalized.is_empty() {
        return PAPERS_DIR.to_string();
    }
    if document_kind_from_relative(&normalized).is_some() && normalized.contains('/') {
        return canonical_library_prefix(&normalized);
    }
    format!("{PAPERS_DIR}/{normalized}")
}

fn canonical_library_prefix(relative: &str) -> String {
    let normalized = normalize_slashes(relative);
    let mut parts = normalized.split('/');
    let Some(first) = parts.next() else {
        return PAPERS_DIR.to_string();
    };
    let rest: Vec<&str> = parts.filter(|part| !part.is_empty()).collect();
    let dir = DocumentKind::from_dir_name(first)
        .map(DocumentKind::dir_name)
        .unwrap_or(first);
    if rest.is_empty() {
        dir.to_string()
    } else {
        format!("{dir}/{}", rest.join("/"))
    }
}

/// Treat missing kind as Papers (legacy import/move collection argument).
pub fn canonicalize_collection_argument(collection: Option<&str>) -> Result<PathBuf, String> {
    let raw = collection.map(str::trim).filter(|value| !value.is_empty());
    let prefixed = match raw {
        None => format!("{PAPERS_DIR}/Inbox"),
        Some(value) if looks_like_library_relative(value) => canonical_library_prefix(value),
        Some(value) => format!("{PAPERS_DIR}/{}", normalize_slashes(value)),
    };
    safe_library_relative(Path::new(&prefixed))
}

pub fn safe_library_relative(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Err("Path must be relative to the Workspace".to_string());
    }
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let name = value.to_string_lossy();
                if name == "." || name == ".." {
                    return Err("Path contains an unsafe component".to_string());
                }
                safe.push(value);
            }
            _ => return Err("Path contains an unsafe component".to_string()),
        }
    }
    if safe.as_os_str().is_empty() {
        return Err("Path must be under Papers or Textbooks".to_string());
    }
    let first = safe
        .components()
        .next()
        .and_then(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .ok_or_else(|| "Path must be under Papers or Textbooks".to_string())?;
    if DocumentKind::from_dir_name(&first).is_none() {
        return Err("Path must be under Papers or Textbooks".to_string());
    }
    Ok(safe)
}

pub fn ensure_inside_library(workspace_root: &Path, path: &Path) -> Result<(), String> {
    if !path.starts_with(workspace_root) {
        return Err("Path escaped the Workspace".to_string());
    }
    let relative = path
        .strip_prefix(workspace_root)
        .map_err(|_| "Path escaped the Workspace".to_string())?;
    safe_library_relative(relative)?;
    Ok(())
}

pub fn join_workspace_relative(workspace_root: &Path, relative: &str) -> Result<PathBuf, String> {
    let safe = safe_library_relative(Path::new(&normalize_slashes(relative)))?;
    let path = workspace_root.join(&safe);
    ensure_inside_library(workspace_root, &path)?;
    Ok(path)
}

pub fn library_relative_from_absolute(
    workspace_root: &Path,
    path: &Path,
) -> Result<PathBuf, String> {
    let relative = path
        .strip_prefix(workspace_root)
        .map_err(|_| "Path escaped the Workspace".to_string())?;
    safe_library_relative(relative)
}

pub fn find_unmanaged_pdfs(workspace_root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(workspace_root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_reserved_workspace_entry(&name) {
            continue;
        }
        if file_type.is_file() && is_pdf(&path) {
            found.push(path);
        } else if file_type.is_dir() {
            collect_pdfs_into(&path, &mut found);
        }
    }
    found.sort();
    found
}

pub fn unmanaged_pdf_notice(workspace_root: &Path) -> Option<String> {
    let found = find_unmanaged_pdfs(workspace_root);
    if found.is_empty() {
        return None;
    }
    let total = found.len();
    let listed: Vec<String> = found
        .iter()
        .take(UNMANAGED_LIST_CAP)
        .filter_map(|path| {
            path.strip_prefix(workspace_root)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    let extra = total.saturating_sub(listed.len());
    let mut message = format!(
        "未管理的 PDF，请移入 Papers/ 或 Textbooks/：{}",
        listed.join("、")
    );
    if extra > 0 {
        message.push_str(&format!(" 等共 {total} 个"));
    }
    Some(message)
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
}

fn collect_pdfs_into(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_pdfs_into(&path, files);
        } else if is_pdf(&path) {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_legacy_inbox_paths() {
        assert_eq!(prefix_legacy_relative("Inbox/a.pdf"), "Papers/Inbox/a.pdf");
        assert_eq!(prefix_legacy_relative(""), "Papers");
        assert_eq!(
            prefix_legacy_relative("Papers/Inbox/a.pdf"),
            "Papers/Inbox/a.pdf"
        );
        assert_eq!(
            prefix_legacy_relative("Textbooks/CLRS/ch3.pdf"),
            "Textbooks/CLRS/ch3.pdf"
        );
        assert_eq!(prefix_legacy_relative("papers/x.pdf"), "Papers/x.pdf");
        assert_eq!(prefix_legacy_relative("Papers"), "Papers/Papers");
    }

    #[test]
    fn collection_argument_defaults_to_papers_inbox() {
        let path = canonicalize_collection_argument(None).expect("default");
        assert_eq!(path, PathBuf::from("Papers/Inbox"));
        let nested = canonicalize_collection_argument(Some("ML/Vision")).expect("nested");
        assert_eq!(nested, PathBuf::from("Papers/ML/Vision"));
        let textbook = canonicalize_collection_argument(Some("Textbooks/CLRS")).expect("textbook");
        assert_eq!(textbook, PathBuf::from("Textbooks/CLRS"));
    }

    #[test]
    fn rejects_export_and_internal_paths() {
        assert!(safe_library_relative(Path::new("export/a.pdf")).is_err());
        assert!(safe_library_relative(Path::new(".read-desktop/a.pdf")).is_err());
        assert!(safe_library_relative(Path::new("../Papers/a.pdf")).is_err());
    }

    #[test]
    fn kind_from_relative_path() {
        assert_eq!(
            document_kind_from_relative("Papers/Inbox/a.pdf"),
            Some(DocumentKind::Paper)
        );
        assert_eq!(
            document_kind_from_relative("Textbooks/CLRS/ch3.pdf"),
            Some(DocumentKind::Textbook)
        );
        assert_eq!(document_kind_from_relative("Notes/a.pdf"), None);
    }
}
