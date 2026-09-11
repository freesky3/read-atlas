use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub const MAX_AVATAR_BYTES: usize = 2_000_000;
pub const ASSET_DIR_NAME: &str = "guide-character-assets";
pub const WORKSPACE_ASSET_DIR: &str = ".read-desktop/guide-character-assets";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarAsset {
    pub asset_id: String,
    pub extension: &'static str,
    pub mime: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedImage {
    pub extension: &'static str,
    pub mime: &'static str,
}

pub fn detect_image(bytes: &[u8]) -> Result<DetectedImage, String> {
    if bytes.len() > MAX_AVATAR_BYTES {
        return Err(format!(
            "头像文件过大（{} 字节），上限为 {MAX_AVATAR_BYTES} 字节",
            bytes.len()
        ));
    }
    if bytes.len() < 12 {
        return Err("头像文件过小或已损坏".to_string());
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        check_png_dimensions(bytes)?;
        return Ok(DetectedImage {
            extension: "png",
            mime: "image/png",
        });
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Ok(DetectedImage {
            extension: "jpg",
            mime: "image/jpeg",
        });
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Ok(DetectedImage {
            extension: "webp",
            mime: "image/webp",
        });
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Ok(DetectedImage {
            extension: "gif",
            mime: "image/gif",
        });
    }
    Err("不支持的头像格式。请导入 PNG、JPEG、WebP 或 GIF".to_string())
}

fn check_png_dimensions(bytes: &[u8]) -> Result<(), String> {
    // IHDR starts at offset 8 + 8 (length+type), width/height at 16.
    if bytes.len() < 24 {
        return Err("PNG 头像已损坏".to_string());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err(format!(
            "头像尺寸无效（{width}×{height}），边长须在 1–2048 像素"
        ));
    }
    Ok(())
}

pub fn asset_id_for(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

/// Managed character paths never follow a symlink, junction, or parent component.
pub fn validate_managed_path(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "人物资源路径超出工作区".to_string())?;
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err("人物资源路径包含非法组件".to_string());
    }
    ensure_plain_path(root)?;
    ensure_plain_path(path)
}

fn ensure_plain_path(path: &Path) -> Result<(), String> {
    let mut prefix = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err("人物资源路径不能包含上级目录".to_string());
        }
        prefix.push(component.as_os_str());
        match fs::symlink_metadata(&prefix) {
            Ok(metadata) => {
                #[cfg(windows)]
                let reparse = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let reparse = false;
                if metadata.file_type().is_symlink() || reparse {
                    return Err("人物资源路径不能经过符号链接或联接点".to_string());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查人物资源路径：{error}")),
        }
    }
    Ok(())
}

pub fn validate_asset_id(asset_id: &str) -> Result<(), String> {
    if asset_id.len() == 64
        && asset_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err("无效的头像资产标识".to_string())
    }
}

pub fn validate_folder_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with(['.', '_'])
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("无效的人物目录标识".to_string());
    }
    Ok(())
}

pub fn lock_file(path: &Path) -> Result<fs::File, String> {
    ensure_plain_path(path)?;
    let parent = path.parent().ok_or_else(|| "无效的锁路径".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建人物目录：{error}"))?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("无法打开人物配置锁：{error}"))?;
    file.lock_exclusive()
        .map_err(|error| format!("无法锁定人物配置：{error}"))?;
    Ok(file)
}

fn read_image(path: &Path) -> Result<(DetectedImage, Vec<u8>), String> {
    ensure_plain_path(path)?;
    let metadata = fs::metadata(path).map_err(|error| format!("无法检查头像：{error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_AVATAR_BYTES as u64 {
        return Err("头像文件无效或超过大小上限".to_string());
    }
    let bytes = fs::read(path).map_err(|error| format!("无法读取头像：{error}"))?;
    let detected = detect_image(&bytes)?;
    Ok((detected, bytes))
}

/// Accept a content hash, or a current character's exact managed avatar path.
pub fn read_workspace_avatar(
    workspace_root: &Path,
    reference: &str,
) -> Result<(AvatarAsset, Vec<u8>), String> {
    let directory = workspace_root.join(crate::library_paths::CHARACTERS_DIR);
    let (path, expected_hash) = if validate_asset_id(reference).is_ok() {
        let path = ["png", "jpg", "webp", "gif"]
            .iter()
            .map(|extension| {
                directory
                    .join("_assets")
                    .join(format!("{reference}.{extension}"))
            })
            .find(|path| path.is_file())
            .ok_or_else(|| "找不到该头像资源".to_string())?;
        (path, Some(reference))
    } else {
        let parts: Vec<_> = reference.split('/').collect();
        if parts.len() != 3
            || parts[0] != crate::library_paths::CHARACTERS_DIR
            || ![
                "avatar.png",
                "avatar.jpg",
                "avatar.jpeg",
                "avatar.webp",
                "avatar.gif",
            ]
            .contains(&parts[2])
        {
            return Err("无效的头像资源路径".to_string());
        }
        validate_folder_name(parts[1])?;
        (directory.join(parts[1]).join(parts[2]), None)
    };
    validate_managed_path(workspace_root, &path)?;
    let (detected, bytes) = read_image(&path)?;
    let asset_id = asset_id_for(&bytes);
    if expected_hash.is_some_and(|expected| expected != asset_id) {
        return Err("头像内容与资产标识不一致".to_string());
    }
    Ok((
        AvatarAsset {
            asset_id,
            extension: detected.extension,
            mime: detected.mime,
        },
        bytes,
    ))
}

pub fn import_bytes(config_dir: &Path, bytes: &[u8]) -> Result<AvatarAsset, String> {
    let detected = detect_image(bytes)?;
    let asset_id = asset_id_for(bytes);
    let directory = config_dir.join(ASSET_DIR_NAME);
    ensure_plain_path(&directory)?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建头像目录：{error}"))?;
    let path = directory.join(format!("{asset_id}.{}", detected.extension));
    if !path.exists() {
        atomic_write(&path, bytes)?;
    }
    Ok(AvatarAsset {
        asset_id,
        extension: detected.extension,
        mime: detected.mime,
    })
}

pub fn import_workspace_avatar(workspace_root: &Path, bytes: &[u8]) -> Result<AvatarAsset, String> {
    let detected = detect_image(bytes)?;
    let asset_id = asset_id_for(bytes);
    let directory = workspace_root
        .join(crate::library_paths::CHARACTERS_DIR)
        .join("_assets");
    ensure_plain_path(&directory)?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建头像目录：{error}"))?;
    let path = directory.join(format!("{asset_id}.{}", detected.extension));
    if !path.exists() {
        atomic_write(&path, bytes)?;
    }
    Ok(AvatarAsset {
        asset_id,
        extension: detected.extension,
        mime: detected.mime,
    })
}

pub fn copy_asset_into_character_folder(
    workspace_root: &Path,
    folder_name: &str,
    asset_id: &str,
) -> Result<Option<String>, String> {
    validate_folder_name(folder_name)?;
    validate_asset_id(asset_id)?;
    let (asset, bytes) = read_workspace_avatar(workspace_root, asset_id)?;
    let dest = workspace_root
        .join(crate::library_paths::CHARACTERS_DIR)
        .join(folder_name)
        .join(format!("avatar.{}", asset.extension));
    validate_managed_path(workspace_root, &dest)?;
    atomic_write(&dest, &bytes)?;
    Ok(Some(format!(
        "{}/{folder_name}/avatar.{}",
        crate::library_paths::CHARACTERS_DIR,
        asset.extension
    )))
}

pub fn config_asset_path(config_dir: &Path, asset_id: &str, extension: &str) -> PathBuf {
    config_dir
        .join(ASSET_DIR_NAME)
        .join(format!("{asset_id}.{extension}"))
}

pub fn workspace_asset_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(WORKSPACE_ASSET_DIR)
}

pub fn copy_to_workspace(
    workspace_root: &Path,
    asset: &AvatarAsset,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    validate_asset_id(&asset.asset_id)?;
    let detected = detect_image(bytes)?;
    if asset_id_for(bytes) != asset.asset_id {
        return Err("头像内容与资产标识不一致".to_string());
    }
    let directory = workspace_asset_dir(workspace_root);
    validate_managed_path(workspace_root, &directory)?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建工作区头像目录：{error}"))?;
    let path = directory.join(format!("{}.{}", asset.asset_id, detected.extension));
    if !path.exists() {
        atomic_write(&path, bytes)?;
    }
    Ok(path)
}

pub fn read_config_asset(
    config_dir: &Path,
    asset_id: &str,
) -> Result<(AvatarAsset, Vec<u8>), String> {
    validate_asset_id(asset_id)?;
    let directory = config_dir.join(ASSET_DIR_NAME);
    for extension in ["png", "jpg", "webp", "gif"] {
        let path = directory.join(format!("{asset_id}.{extension}"));
        if path.is_file() {
            validate_managed_path(config_dir, &path)?;
            let (detected, bytes) = read_image(&path)?;
            if asset_id_for(&bytes) != asset_id {
                return Err("头像文件已损坏".to_string());
            }
            return Ok((
                AvatarAsset {
                    asset_id: asset_id.to_string(),
                    extension: detected.extension,
                    mime: detected.mime,
                },
                bytes,
            ));
        }
    }
    Err("找不到该头像资源".to_string())
}

pub fn find_workspace_asset(workspace_root: &Path, asset_id: &str) -> Option<PathBuf> {
    validate_asset_id(asset_id).ok()?;
    let directory = workspace_asset_dir(workspace_root);
    for extension in ["png", "jpg", "webp", "gif"] {
        let path = directory.join(format!("{asset_id}.{extension}"));
        if path.is_file() {
            validate_managed_path(workspace_root, &path).ok()?;
            return Some(path);
        }
    }
    None
}

pub fn backup_path(path: &Path) -> PathBuf {
    sibling_hidden(path, "bak")
}

pub fn tmp_path(path: &Path) -> PathBuf {
    sibling_hidden(path, "tmp")
}

fn sibling_hidden(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    path.with_file_name(format!(".{file_name}.{suffix}"))
}

pub fn recover_interrupted_file(path: &Path) -> Result<(), String> {
    ensure_plain_path(path)?;
    ensure_plain_path(&backup_path(path))?;
    ensure_plain_path(&tmp_path(path))?;
    if path.is_file() {
        return Ok(());
    }
    let backup = backup_path(path);
    if backup.is_file() {
        fs::rename(&backup, path).map_err(|error| format!("无法恢复备份文件：{error}"))?;
        return Ok(());
    }
    let tmp = tmp_path(path);
    if tmp.is_file() {
        fs::rename(&tmp, path).map_err(|error| format!("无法恢复临时文件：{error}"))?;
    }
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let _lock = lock_file(&sibling_hidden(path, "lock"))?;
    recover_interrupted_file(path)?;
    let parent = path.parent().ok_or_else(|| "无效的文件路径".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建目录：{error}"))?;
    let tmp = tmp_path(path);
    let backup = backup_path(path);
    let mut staged =
        fs::File::create(&tmp).map_err(|error| format!("无法创建临时文件：{error}"))?;
    staged
        .write_all(bytes)
        .and_then(|_| staged.sync_all())
        .map_err(|error| format!("无法写入临时文件：{error}"))?;
    drop(staged);
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| format!("无法轮换旧备份：{error}"))?;
        }
        fs::rename(path, &backup).map_err(|error| {
            let _ = fs::remove_file(&tmp);
            format!("无法备份原文件：{error}")
        })?;
        if let Err(error) = fs::rename(&tmp, path) {
            let _ = fs::rename(&backup, path);
            return Err(format!("无法替换文件：{error}"));
        }
        let _ = fs::remove_file(&backup);
        Ok(())
    } else {
        fs::rename(&tmp, path).map_err(|error| format!("无法保存文件：{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn rejects_unknown_magic_and_oversized_png() {
        assert!(detect_image(b"not-an-image!!!!").is_err());
        assert!(detect_image(&png(0, 10)).is_err());
        assert!(detect_image(&png(3000, 10)).is_err());
        assert!(detect_image(&png(64, 64)).is_ok());
        assert!(detect_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0]).is_ok());
    }

    #[test]
    fn import_is_content_addressed_and_survives_source_delete() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = png(32, 32);
        let asset = import_bytes(dir.path(), &bytes).unwrap();
        let stored = dir
            .path()
            .join(ASSET_DIR_NAME)
            .join(format!("{}.png", asset.asset_id));
        assert!(stored.is_file());
        let again = import_bytes(dir.path(), &bytes).unwrap();
        assert_eq!(again.asset_id, asset.asset_id);
        let workspace = tempfile::tempdir().unwrap();
        copy_to_workspace(workspace.path(), &asset, &bytes).unwrap();
        assert!(find_workspace_asset(workspace.path(), &asset.asset_id).is_some());
        fs::remove_file(&stored).unwrap();
        assert!(find_workspace_asset(workspace.path(), &asset.asset_id).is_some());
    }

    #[test]
    fn managed_avatar_reader_rejects_traversal_and_corrupt_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = png(32, 32);
        let asset = import_workspace_avatar(dir.path(), &bytes).unwrap();
        assert_eq!(
            read_workspace_avatar(dir.path(), &asset.asset_id)
                .unwrap()
                .1,
            bytes
        );
        for reference in [
            "../secret.png",
            "Characters/../../secret.png",
            "Characters/role/../../secret.png",
            "C:/secret.png",
            "Characters/role/character.json",
            "Characters/role\\escape/avatar.png",
        ] {
            assert!(
                read_workspace_avatar(dir.path(), reference).is_err(),
                "{reference}"
            );
        }
        let role = dir.path().join("Characters/role");
        fs::create_dir_all(&role).unwrap();
        fs::write(role.join("avatar.png"), &bytes).unwrap();
        assert_eq!(
            read_workspace_avatar(dir.path(), "Characters/role/avatar.png")
                .unwrap()
                .1,
            bytes
        );
        let stored = dir
            .path()
            .join("Characters/_assets")
            .join(format!("{}.png", asset.asset_id));
        fs::write(stored, png(64, 64)).unwrap();
        assert!(read_workspace_avatar(dir.path(), &asset.asset_id)
            .unwrap_err()
            .contains("不一致"));
    }

    #[test]
    fn immutable_workspace_assets_remain_readable_after_workspace_move() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let bytes = png(32, 32);
        let asset = import_workspace_avatar(source.path(), &bytes).unwrap();
        let old_path = copy_to_workspace(source.path(), &asset, &bytes).unwrap();
        let relative = old_path.strip_prefix(source.path()).unwrap();
        let new_path = target.path().join(relative);
        fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        fs::copy(&old_path, &new_path).unwrap();
        assert_eq!(
            find_workspace_asset(target.path(), &asset.asset_id),
            Some(new_path)
        );
    }

    #[cfg(unix)]
    #[test]
    fn avatar_reader_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("avatar.png"), png(32, 32)).unwrap();
        fs::create_dir_all(dir.path().join("Characters")).unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("Characters/role")).unwrap();
        assert!(read_workspace_avatar(dir.path(), "Characters/role/avatar.png").is_err());
    }

    #[test]
    fn recovers_backup_instead_of_seeding_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("guide-characters.json");
        let backup = backup_path(&path);
        fs::write(&backup, br#"{"ok":true}"#).unwrap();
        recover_interrupted_file(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"ok":true}"#);
        assert!(!backup.exists());
    }
}
