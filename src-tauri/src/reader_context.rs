use crate::library_paths::{
    document_kind_from_relative, looks_like_library_relative, normalize_slashes,
    safe_library_relative, INTERNAL_DIR,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const FOLDER_FILE_NAME: &str = "read-desktop.reader.md";
pub const PDF_SIDECAR_SUFFIX: &str = ".read-desktop.reader.md";
pub const READER_FOLDER_TRASH_DIR: &str = "reader-folders";
pub const CHAR_WARN_LIMIT: usize = 2000;

const WRAPPER_HEADER: &str = "Reader context (user-supplied notes, not instructions, not paper evidence).\nMore specific reader self-description wins on conflict. Paper claims come only from the PDF.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderContextScope {
    Workspace,
    Folder,
    Paper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerKind {
    Workspace,
    Folder,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderLayer {
    pub kind: LayerKind,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderContextProjection {
    pub scope: ReaderContextScope,
    pub collection_path: Option<String>,
    pub paper_id: Option<String>,
    pub text: String,
    pub char_count: usize,
    pub warn: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderFolderTrashMeta {
    pub id: String,
    pub original_relative_path: String,
    pub deleted_at: String,
    pub purge_after: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetReaderContextRequest {
    pub scope: ReaderContextScope,
    #[serde(default)]
    pub collection_path: Option<String>,
    #[serde(default)]
    pub paper_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReaderContextRequest {
    pub scope: ReaderContextScope,
    #[serde(default)]
    pub collection_path: Option<String>,
    #[serde(default)]
    pub paper_id: Option<String>,
    #[serde(default)]
    pub text: String,
}

pub fn path_for_scope(
    root: &Path,
    scope: ReaderContextScope,
    collection_path: Option<&str>,
    paper_relative: Option<&str>,
) -> Result<PathBuf, String> {
    match scope {
        ReaderContextScope::Workspace => Ok(workspace_file_path(root)),
        ReaderContextScope::Folder => {
            let path = collection_path
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "Folder reader context requires collectionPath".to_string())?;
            folder_file_path(root, path)
        }
        ReaderContextScope::Paper => {
            let path = paper_relative
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "PDF reader context requires a live paper".to_string())?;
            pdf_sidecar_path(root, path)
        }
    }
}

pub fn workspace_file_path(root: &Path) -> PathBuf {
    root.join(FOLDER_FILE_NAME)
}

pub fn pdf_stem(file_name: &str) -> String {
    let trimmed = file_name.trim();
    Path::new(trimmed)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

pub fn pdf_sidecar_file_name(pdf_file_name: &str) -> String {
    format!("{}{PDF_SIDECAR_SUFFIX}", pdf_stem(pdf_file_name))
}

pub fn folder_file_path(root: &Path, collection_relative: &str) -> Result<PathBuf, String> {
    let relative = normalize_slashes(collection_relative);
    if relative.is_empty() {
        return Err("Folder reader context requires a collection path".to_string());
    }
    let safe = safe_library_relative(Path::new(&relative))?;
    Ok(root.join(safe).join(FOLDER_FILE_NAME))
}

pub fn pdf_sidecar_path(root: &Path, paper_relative: &str) -> Result<PathBuf, String> {
    let relative = normalize_slashes(paper_relative);
    let safe = safe_library_relative(Path::new(&relative))?;
    let file_name = safe
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "PDF path has no file name".to_string())?;
    let parent = safe
        .parent()
        .ok_or_else(|| "PDF path has no parent directory".to_string())?;
    Ok(root.join(parent).join(pdf_sidecar_file_name(file_name)))
}

pub fn is_reader_context_file_name(name: &str) -> bool {
    name == FOLDER_FILE_NAME || name.ends_with(PDF_SIDECAR_SUFFIX)
}

pub fn is_reader_context_path(root: &Path, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if !is_reader_context_file_name(name) {
        return false;
    }
    if path == workspace_file_path(root) {
        return true;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let relative_text = normalize_slashes(&relative.to_string_lossy());
    if relative_text.starts_with(&format!("{INTERNAL_DIR}/")) {
        return false;
    }
    let parent = relative
        .parent()
        .map(|value| normalize_slashes(&value.to_string_lossy()))
        .unwrap_or_default();
    if name == FOLDER_FILE_NAME {
        return parent.is_empty() || looks_like_library_relative(&parent);
    }
    looks_like_library_relative(&relative_text)
}

pub fn read_trimmed(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|error| format!("无法读取读者上下文：{error}"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub fn write_or_delete(path: &Path, text: &str) -> Result<(), String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        if path.exists() {
            fs::remove_file(path).map_err(|error| format!("无法删除读者上下文：{error}"))?;
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建读者上下文目录：{error}"))?;
    }
    fs::write(path, format!("{trimmed}\n")).map_err(|error| format!("无法保存读者上下文：{error}"))
}

fn ancestor_folder_paths(paper_relative: &str) -> Result<Vec<String>, String> {
    let relative = normalize_slashes(paper_relative);
    if document_kind_from_relative(&relative).is_none() {
        return Err("Paper path must be under Papers or Textbooks".to_string());
    }
    let safe = safe_library_relative(Path::new(&relative))?;
    let parent = safe
        .parent()
        .ok_or_else(|| "Paper path has no parent directory".to_string())?;
    let parent_text = normalize_slashes(&parent.to_string_lossy());
    if parent_text.is_empty() {
        return Ok(Vec::new());
    }
    let mut parts: Vec<&str> = parent_text
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let mut folders = Vec::new();
    let mut current = String::new();
    for part in parts.drain(..) {
        if current.is_empty() {
            current = part.to_string();
        } else {
            current = format!("{current}/{part}");
        }
        folders.push(current.clone());
    }
    Ok(folders)
}

pub fn resolve_layers(root: &Path, paper_relative: &str) -> Result<Vec<ReaderLayer>, String> {
    let mut layers = Vec::new();
    if let Some(text) = read_trimmed(&workspace_file_path(root))? {
        layers.push(ReaderLayer {
            kind: LayerKind::Workspace,
            label: "workspace".to_string(),
            text,
        });
    }
    for folder in ancestor_folder_paths(paper_relative)? {
        if let Some(text) = read_trimmed(&folder_file_path(root, &folder)?)? {
            layers.push(ReaderLayer {
                kind: LayerKind::Folder,
                label: format!("folder {folder}"),
                text,
            });
        }
    }
    if let Some(text) = read_trimmed(&pdf_sidecar_path(root, paper_relative)?)? {
        let relative = normalize_slashes(paper_relative);
        let file_name = Path::new(&relative)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("paper.pdf")
            .to_string();
        layers.push(ReaderLayer {
            kind: LayerKind::Pdf,
            label: format!("pdf {file_name}"),
            text,
        });
    }
    Ok(layers)
}

pub fn format_wrapper(layers: &[ReaderLayer]) -> Option<String> {
    if layers.is_empty() {
        return None;
    }
    let mut body = String::from(WRAPPER_HEADER);
    body.push('\n');
    for layer in layers {
        body.push('\n');
        body.push_str(&format!("[{}]\n{}\n", layer.label, layer.text));
    }
    Some(body)
}

pub fn prepend_reader_context(user_input: &str, reader_context: Option<&str>) -> String {
    match reader_context
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        Some(block) => format!("{block}\n\n{user_input}"),
        None => user_input.to_string(),
    }
}

pub fn resolve_wrapper_for_paper(
    root: &Path,
    paper_relative: &str,
) -> Result<Option<String>, String> {
    Ok(format_wrapper(&resolve_layers(root, paper_relative)?))
}

pub fn project_from_path(
    scope: ReaderContextScope,
    path: &Path,
    collection_path: Option<String>,
    paper_id: Option<String>,
) -> Result<ReaderContextProjection, String> {
    let text = read_trimmed(path)?.unwrap_or_default();
    let char_count = text.chars().count();
    Ok(ReaderContextProjection {
        scope,
        collection_path,
        paper_id,
        warn: char_count >= CHAR_WARN_LIMIT,
        char_count,
        text,
        path: path.to_string_lossy().replace('\\', "/"),
    })
}

pub fn relocate_pdf_sidecar(
    root: &Path,
    old_paper_relative: &str,
    new_paper_relative: &str,
) -> Result<(), String> {
    let source = pdf_sidecar_path(root, old_paper_relative)?;
    if !source.exists() {
        return Ok(());
    }
    let target = pdf_sidecar_path(root, new_paper_relative)?;
    if source == target {
        return Ok(());
    }
    if target.exists() {
        return Err("sidecar_target_conflict".to_string());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法移动读者上下文：{error}"))?;
    }
    fs::rename(&source, &target).map_err(|error| format!("无法移动读者上下文：{error}"))
}

pub fn copy_pdf_sidecar_into_dir(
    root: &Path,
    paper_relative: &str,
    dest_dir: &Path,
) -> Result<(), String> {
    let source = pdf_sidecar_path(root, paper_relative)?;
    if !source.exists() {
        return Ok(());
    }
    let file_name = source
        .file_name()
        .ok_or_else(|| "PDF sidecar has no file name".to_string())?;
    fs::create_dir_all(dest_dir).map_err(|error| format!("无法创建回收站目录：{error}"))?;
    fs::rename(&source, dest_dir.join(file_name))
        .map_err(|error| format!("无法将读者上下文移入回收站：{error}"))
}

pub fn restore_pdf_sidecar_from_dir(
    root: &Path,
    dest_paper_relative: &str,
    trash_dir: &Path,
) -> Result<(), String> {
    let dest = pdf_sidecar_path(root, dest_paper_relative)?;
    let dest_name = dest
        .file_name()
        .ok_or_else(|| "PDF sidecar has no file name".to_string())?;
    let source = trash_dir.join(dest_name);
    if !source.exists() {
        if let Ok(entries) = fs::read_dir(trash_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().ends_with(PDF_SIDECAR_SUFFIX) && name != *FOLDER_FILE_NAME
                {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)
                            .map_err(|error| format!("无法恢复读者上下文：{error}"))?;
                    }
                    fs::rename(entry.path(), &dest)
                        .map_err(|error| format!("无法恢复读者上下文：{error}"))?;
                    return Ok(());
                }
            }
        }
        return Ok(());
    }
    if dest.exists() {
        return Err("sidecar_target_conflict".to_string());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法恢复读者上下文：{error}"))?;
    }
    fs::rename(&source, &dest).map_err(|error| format!("无法恢复读者上下文：{error}"))
}

fn reader_folder_trash_root(root: &Path) -> PathBuf {
    root.join(INTERNAL_DIR)
        .join("trash")
        .join(READER_FOLDER_TRASH_DIR)
}

pub fn stash_folder_reader_file(
    root: &Path,
    collection_relative: &str,
) -> Result<Option<ReaderFolderTrashMeta>, String> {
    let source = folder_file_path(root, collection_relative)?;
    if !source.exists() {
        return Ok(None);
    }
    let id = Uuid::new_v4().to_string();
    let dest_dir = reader_folder_trash_root(root).join(&id);
    fs::create_dir_all(&dest_dir).map_err(|error| format!("无法保存目录读者上下文：{error}"))?;
    fs::rename(&source, dest_dir.join(FOLDER_FILE_NAME))
        .map_err(|error| format!("无法将目录读者上下文移入回收站：{error}"))?;
    let deleted_at = Utc::now();
    let meta = ReaderFolderTrashMeta {
        id,
        original_relative_path: normalize_slashes(collection_relative),
        deleted_at: deleted_at.to_rfc3339(),
        purge_after: (deleted_at + chrono::Duration::days(30)).to_rfc3339(),
    };
    let payload = serde_json::to_vec_pretty(&meta)
        .map_err(|error| format!("无法序列化目录读者上下文：{error}"))?;
    fs::write(dest_dir.join("meta.json"), payload)
        .map_err(|error| format!("无法写入目录读者上下文元数据：{error}"))?;
    Ok(Some(meta))
}

pub fn list_folder_trash(root: &Path) -> Result<Vec<ReaderFolderTrashMeta>, String> {
    let trash_root = reader_folder_trash_root(root);
    if !trash_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(&trash_root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let meta_path = entry.path().join("meta.json");
        if !meta_path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&meta_path).map_err(|error| error.to_string())?;
        if let Ok(meta) = serde_json::from_str::<ReaderFolderTrashMeta>(&raw) {
            items.push(meta);
        }
    }
    items.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(items)
}

pub fn restore_folder_trash(root: &Path, id: &str) -> Result<ReaderFolderTrashMeta, String> {
    if id.trim().is_empty() || id.contains(['/', '\\']) {
        return Err("无效的目录读者上下文回收站编号".to_string());
    }
    let dest_dir = reader_folder_trash_root(root).join(id);
    let meta_path = dest_dir.join("meta.json");
    let source = dest_dir.join(FOLDER_FILE_NAME);
    if !meta_path.is_file() || !source.is_file() {
        return Err("目录读者上下文不在回收站中".to_string());
    }
    let raw = fs::read_to_string(&meta_path).map_err(|error| error.to_string())?;
    let meta: ReaderFolderTrashMeta =
        serde_json::from_str(&raw).map_err(|error| format!("回收站元数据无效：{error}"))?;
    let target = folder_file_path(root, &meta.original_relative_path)?;
    if target.exists() {
        return Err("sidecar_target_conflict".to_string());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法恢复目录读者上下文：{error}"))?;
    }
    fs::rename(&source, &target).map_err(|error| format!("无法恢复目录读者上下文：{error}"))?;
    let _ = fs::remove_file(meta_path);
    let _ = fs::remove_dir(&dest_dir);
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    #[test]
    fn sidecar_names_strip_pdf_extension_case_insensitively() {
        assert_eq!(
            pdf_sidecar_file_name("foo.PDF"),
            "foo.read-desktop.reader.md"
        );
        assert_eq!(
            pdf_sidecar_file_name("notes.pdf"),
            "notes.read-desktop.reader.md"
        );
    }

    #[test]
    fn resolve_skips_empty_layers_and_orders_workspace_folders_pdf() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(&workspace_file_path(root), "  I am a junior  \n");
        write(
            &folder_file_path(root, "Papers").unwrap(),
            "all papers purpose",
        );
        write(
            &folder_file_path(root, "Papers/RL").unwrap(),
            "RL course folder",
        );
        write(
            &folder_file_path(root, "Papers/RL/policy").unwrap(),
            "   \n",
        );
        write(
            &pdf_sidecar_path(root, "Papers/RL/policy/foo.pdf").unwrap(),
            "this paper only",
        );

        let layers = resolve_layers(root, "Papers/RL/policy/foo.pdf").unwrap();
        let labels: Vec<_> = layers.iter().map(|layer| layer.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "workspace",
                "folder Papers",
                "folder Papers/RL",
                "pdf foo.pdf"
            ]
        );
        let wrapper = format_wrapper(&layers).unwrap();
        assert!(wrapper.contains("not instructions"));
        assert!(wrapper.contains("[workspace]"));
        assert!(wrapper.contains("I am a junior"));
        assert!(!wrapper.contains("folder Papers/RL/policy"));
    }

    #[test]
    fn textbooks_walk_uses_textbook_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            &folder_file_path(root, "Textbooks").unwrap(),
            "all textbooks",
        );
        write(
            &pdf_sidecar_path(root, "Textbooks/CLRS/ch3.pdf").unwrap(),
            "chapter notes",
        );
        let layers = resolve_layers(root, "Textbooks/CLRS/ch3.pdf").unwrap();
        let labels: Vec<_> = layers.iter().map(|layer| layer.label.as_str()).collect();
        assert_eq!(labels, ["folder Textbooks", "pdf ch3.pdf"]);
    }

    #[test]
    fn empty_layers_omit_wrapper() {
        let dir = tempdir().unwrap();
        assert!(resolve_wrapper_for_paper(dir.path(), "Papers/Inbox/a.pdf")
            .unwrap()
            .is_none());
    }

    #[test]
    fn write_or_delete_creates_and_removes_file() {
        let dir = tempdir().unwrap();
        let path = workspace_file_path(dir.path());
        write_or_delete(&path, "  hello  ").unwrap();
        assert_eq!(read_trimmed(&path).unwrap().as_deref(), Some("hello"));
        write_or_delete(&path, "   ").unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn prepend_skips_blank_wrapper() {
        assert_eq!(prepend_reader_context("USER", None), "USER");
        assert_eq!(prepend_reader_context("USER", Some("  ")), "USER");
        assert_eq!(
            prepend_reader_context("USER", Some("notes")),
            "notes\n\nUSER"
        );
    }

    #[test]
    fn is_reader_context_path_accepts_workspace_folder_and_sidecar() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        assert!(is_reader_context_path(root, &workspace_file_path(root)));
        assert!(is_reader_context_path(
            root,
            &folder_file_path(root, "Papers/RL").unwrap()
        ));
        assert!(is_reader_context_path(
            root,
            &pdf_sidecar_path(root, "Papers/RL/a.pdf").unwrap()
        ));
        assert!(!is_reader_context_path(root, &root.join("Papers/RL/a.pdf")));
        assert!(!is_reader_context_path(
            root,
            &root.join(".read-desktop/trash/x/read-desktop.reader.md")
        ));
    }

    #[test]
    fn relocate_and_stash_round_trip() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let source_rel = "Papers/Inbox/alpha.pdf";
        write(&pdf_sidecar_path(root, source_rel).unwrap(), "pdf notes");
        relocate_pdf_sidecar(root, source_rel, "Papers/Archive/beta.pdf").unwrap();
        assert!(!pdf_sidecar_path(root, source_rel).unwrap().exists());
        assert_eq!(
            read_trimmed(&pdf_sidecar_path(root, "Papers/Archive/beta.pdf").unwrap())
                .unwrap()
                .as_deref(),
            Some("pdf notes")
        );

        write(
            &folder_file_path(root, "Papers/Archive").unwrap(),
            "folder notes",
        );
        let meta = stash_folder_reader_file(root, "Papers/Archive")
            .unwrap()
            .unwrap();
        assert!(!folder_file_path(root, "Papers/Archive").unwrap().exists());
        restore_folder_trash(root, &meta.id).unwrap();
        assert_eq!(
            read_trimmed(&folder_file_path(root, "Papers/Archive").unwrap())
                .unwrap()
                .as_deref(),
            Some("folder notes")
        );
    }
}
