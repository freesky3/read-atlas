//! Shared SQLite connection setup.
//!
//! SQLite is still deliberately opened per operation.  The desktop workload is
//! single-user and WAL-backed, so a pool would add lifecycle and shutdown
//! complexity without improving the common case.  Keeping the setup here makes
//! every module use the same durability and locking settings, though.

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const BUSY_TIMEOUT_MS: i64 = 5_000;

/// Open a normal workspace connection with the settings required by every
/// repository/module.
pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    configure(&connection, false)?;
    Ok(connection)
}

/// Open a connection while also enabling WAL.  This is used only during
/// workspace initialization; normal callers should use [`open`].
pub fn open_wal(path: impl AsRef<Path>) -> rusqlite::Result<Connection> {
    let connection = Connection::open(path)?;
    configure(&connection, true)?;
    Ok(connection)
}

pub fn configure(connection: &Connection, wal: bool) -> rusqlite::Result<()> {
    if wal {
        connection.execute_batch("PRAGMA journal_mode = WAL;")?;
    }
    connection.execute_batch(&format!(
        "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"
    ))?;
    register_chapter_sort_key(connection)
}

fn register_chapter_sort_key(connection: &Connection) -> rusqlite::Result<()> {
    connection.create_scalar_function(
        "chapter_sort_key",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let value: Option<String> = ctx.get(0)?;
            Ok(crate::chapter_sort::chapter_sort_key(value.as_deref()))
        },
    )
}

#[allow(dead_code)]
pub(crate) fn checkpoint_close_copy_database(
    database_path: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    if !database_path.is_file() {
        return Err(format!(
            "Database does not exist or is not a regular file: {}",
            database_path.display()
        ));
    }

    checkpoint_database(database_path)?;

    let parent = database_path
        .parent()
        .ok_or_else(|| "Database path has no parent directory".to_string())?;
    let file_name = database_path
        .file_name()
        .ok_or_else(|| "Database path has no file name".to_string())?;
    let safe_label: String = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let backup_dir = parent.join("backups").join(format!(
        "{}-{}",
        safe_label.trim_matches('-'),
        Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    let backup_path = backup_dir.join(file_name);
    fs::copy(database_path, &backup_path).map_err(|error| error.to_string())?;
    validate_database_copy(&backup_path)?;
    ensure_self_contained_database_copy(&backup_path)?;
    Ok(backup_path)
}

#[allow(dead_code)]
pub(crate) fn restore_database_copy(
    database_path: &Path,
    backup_path: &Path,
) -> Result<(), String> {
    restore_database_copy_inner(database_path, backup_path, || Ok(()))
}

#[cfg(test)]
pub(crate) fn restore_database_copy_with_before_promote_fault(
    database_path: &Path,
    backup_path: &Path,
) -> Result<(), String> {
    restore_database_copy_inner(database_path, backup_path, || {
        Err("injected restore failure before promotion".to_string())
    })
}

fn restore_database_copy_inner<F>(
    database_path: &Path,
    backup_path: &Path,
    before_promote: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    require_regular_file(backup_path, "Backup database")?;
    ensure_self_contained_database_copy(backup_path)?;
    validate_database_copy(backup_path)?;
    ensure_self_contained_database_copy(backup_path)?;

    let token = Uuid::new_v4().simple().to_string();
    let staging_path = restore_sibling_path(database_path, "stage", &token)?;
    let recovery_path = restore_sibling_path(database_path, "recovery", &token)?;
    inspect_regular_file_set(database_path, "Restore target")?;
    inspect_absent_file_set(&staging_path, "Restore staging")?;
    inspect_absent_file_set(&recovery_path, "Restore recovery")?;

    if let Err(error) = fs::copy(backup_path, &staging_path) {
        let _ = remove_regular_file_set(&staging_path);
        return Err(format!("Unable to stage database restore: {error}"));
    }
    if let Err(error) = validate_database_copy(&staging_path)
        .and_then(|_| ensure_self_contained_database_copy(&staging_path))
    {
        let cleanup = remove_regular_file_set(&staging_path);
        return Err(with_cleanup_error(error, cleanup));
    }

    let moved = match move_regular_file_set(database_path, &recovery_path) {
        Ok(moved) => moved,
        Err((error, moved)) => {
            return Err(rollback_before_promotion(&staging_path, &moved, error));
        }
    };

    if let Err(error) = before_promote() {
        return Err(rollback_before_promotion(&staging_path, &moved, error));
    }

    if let Err(error) = fs::rename(&staging_path, database_path) {
        return Err(rollback_before_promotion(
            &staging_path,
            &moved,
            format!("Unable to promote staged database restore: {error}"),
        ));
    }

    if let Err(error) = validate_database_copy(database_path)
        .and_then(|_| ensure_self_contained_database_copy(database_path))
    {
        if let Err(move_error) = fs::rename(database_path, &staging_path) {
            return Err(format!(
                "{error}; restored candidate could not be quarantined: {move_error}; original file set remains at {}",
                recovery_path.display()
            ));
        }
        return Err(rollback_before_promotion(&staging_path, &moved, error));
    }

    for (_, recovery_member) in &moved {
        fs::remove_file(recovery_member).map_err(|error| {
            format!(
                "Restore succeeded but old database recovery member could not be removed at {}: {error}",
                recovery_member.display()
            )
        })?;
    }
    ensure_self_contained_database_copy(backup_path)
}

fn restore_sibling_path(database_path: &Path, role: &str, token: &str) -> Result<PathBuf, String> {
    let parent = database_path
        .parent()
        .ok_or_else(|| "Restore target has no parent directory".to_string())?;
    let file_name = database_path
        .file_name()
        .ok_or_else(|| "Restore target has no file name".to_string())?;
    let mut sibling_name = file_name.to_os_string();
    sibling_name.push(format!(".restore-{role}-{token}"));
    Ok(parent.join(sibling_name))
}

fn inspect_regular_file_set(database_path: &Path, label: &str) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let member = database_file_set_member(database_path, suffix);
        match fs::symlink_metadata(&member) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(format!(
                    "{label} member is not a regular file: {}",
                    member.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect {label} member {}: {error}",
                    member.display()
                ));
            }
        }
    }
    Ok(())
}

fn inspect_absent_file_set(database_path: &Path, label: &str) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let member = database_file_set_member(database_path, suffix);
        match fs::symlink_metadata(&member) {
            Ok(_) => {
                return Err(format!(
                    "{label} member already exists: {}",
                    member.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect {label} member {}: {error}",
                    member.display()
                ));
            }
        }
    }
    Ok(())
}

fn move_regular_file_set(
    source_database: &Path,
    destination_database: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, (String, Vec<(PathBuf, PathBuf)>)> {
    let mut moved = Vec::new();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let source = database_file_set_member(source_database, suffix);
        match fs::symlink_metadata(&source) {
            Ok(metadata) if metadata.file_type().is_file() => {
                let destination = database_file_set_member(destination_database, suffix);
                if let Err(error) = fs::rename(&source, &destination) {
                    return Err((
                        format!(
                            "Unable to preserve restore target member {}: {error}",
                            source.display()
                        ),
                        moved,
                    ));
                }
                moved.push((source, destination));
            }
            Ok(_) => {
                return Err((
                    format!(
                        "Restore target member is not a regular file: {}",
                        source.display()
                    ),
                    moved,
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err((
                    format!(
                        "Could not inspect restore target member {}: {error}",
                        source.display()
                    ),
                    moved,
                ));
            }
        }
    }
    Ok(moved)
}

fn rollback_before_promotion(
    staging_path: &Path,
    moved: &[(PathBuf, PathBuf)],
    primary_error: String,
) -> String {
    let mut cleanup_errors = Vec::new();
    for (original, recovery) in moved.iter().rev() {
        if let Err(error) = fs::rename(recovery, original) {
            cleanup_errors.push(format!(
                "could not restore {} from {}: {error}",
                original.display(),
                recovery.display()
            ));
        }
    }
    if let Err(error) = remove_regular_file_set(staging_path) {
        cleanup_errors.push(error);
    }
    if cleanup_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error}; restore rollback incomplete: {}",
            cleanup_errors.join("; ")
        )
    }
}

fn remove_regular_file_set(database_path: &Path) -> Result<(), String> {
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let member = database_file_set_member(database_path, suffix);
        match fs::symlink_metadata(&member) {
            Ok(metadata) if metadata.file_type().is_file() => {
                fs::remove_file(&member).map_err(|error| {
                    format!(
                        "Could not remove restore artifact {}: {error}",
                        member.display()
                    )
                })?;
            }
            Ok(_) => {
                return Err(format!(
                    "Restore artifact is not a regular file: {}",
                    member.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect restore artifact {}: {error}",
                    member.display()
                ));
            }
        }
    }
    Ok(())
}

fn with_cleanup_error(primary_error: String, cleanup: Result<(), String>) -> String {
    match cleanup {
        Ok(()) => primary_error,
        Err(cleanup_error) => format!("{primary_error}; cleanup failed: {cleanup_error}"),
    }
}

fn require_regular_file(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(()),
        Ok(_) => Err(format!("{label} is not a regular file: {}", path.display())),
        Err(error) => Err(format!(
            "Could not inspect {label} {}: {error}",
            path.display()
        )),
    }
}

fn database_file_set_member(database_path: &Path, suffix: &str) -> PathBuf {
    if suffix.is_empty() {
        database_path.to_path_buf()
    } else {
        database_sidecar_path(database_path, suffix)
    }
}

fn checkpoint_database(database_path: &Path) -> Result<(), String> {
    let connection = open(database_path).map_err(|error| error.to_string())?;
    let (busy, _log_frames, _checkpointed_frames): (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| error.to_string())?;
    if busy != 0 {
        return Err(format!(
            "Database checkpoint remained busy for {}",
            database_path.display()
        ));
    }
    drop(connection);
    Ok(())
}

fn validate_database_copy(database_path: &Path) -> Result<(), String> {
    if !database_path.is_file() {
        return Err(format!(
            "Database copy does not exist or is not a regular file: {}",
            database_path.display()
        ));
    }
    let connection = open_immutable_database_copy(database_path)?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| error.to_string())?;
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!(
            "Database quick_check failed for {}: {quick_check}",
            database_path.display()
        ));
    }
    let foreign_key_errors: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    if foreign_key_errors != 0 {
        return Err(format!(
            "Database copy has {foreign_key_errors} foreign key violations: {}",
            database_path.display()
        ));
    }
    Ok(())
}

fn open_immutable_database_copy(database_path: &Path) -> Result<Connection, String> {
    let absolute_path = if database_path.is_absolute() {
        database_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(database_path)
    };
    let mut database_uri = reqwest::Url::from_file_path(&absolute_path).map_err(|_| {
        format!(
            "Database copy path cannot be represented as a file URI: {}",
            database_path.display()
        )
    })?;
    database_uri.query_pairs_mut().append_pair("immutable", "1");
    Connection::open_with_flags(
        database_uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())
}

fn ensure_self_contained_database_copy(database_path: &Path) -> Result<(), String> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = database_sidecar_path(database_path, suffix);
        match fs::symlink_metadata(&sidecar) {
            Ok(_) => {
                return Err(format!(
                    "Backup database is not self-contained: unexpected {suffix} sidecar at {}",
                    sidecar.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect backup sidecar {}: {error}",
                    sidecar.display()
                ));
            }
        }
    }
    Ok(())
}

fn database_sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut name = database_path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
