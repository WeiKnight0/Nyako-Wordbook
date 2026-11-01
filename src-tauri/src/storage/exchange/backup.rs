use super::{
    encode,
    format::{id, json_error, strict_json},
};
use crate::{
    error::AppError,
    storage::{libraries::now, migrations},
};
use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    time::Duration,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MAX_EXPANDED: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    format: String,
    version: u32,
    schema_version: usize,
    app_version: String,
    created_at: i64,
    words: i64,
    events: i64,
    media: usize,
    files: Vec<Entry>,
}

fn invalid(message: &str) -> AppError {
    AppError::new("INVALID_BACKUP", message)
}

fn file_hash(path: &Path) -> Result<(u64, String), AppError> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    let mut length = 0;
    loop {
        let size = input.read(&mut bytes)?;
        if size == 0 {
            break;
        }
        digest.update(&bytes[..size]);
        length += size as u64;
    }
    Ok((
        length,
        digest
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    ))
}

fn copy_controlled(
    input: &mut impl Read,
    output: &mut impl Write,
    control: &super::jobs::Control,
) -> Result<u64, AppError> {
    let mut buffer = [0; 65536];
    let mut total = 0;
    loop {
        control.check()?;
        let length = input.read(&mut buffer)?;
        if length == 0 {
            return Ok(total);
        }
        output.write_all(&buffer[..length])?;
        total += length as u64;
    }
}

fn safe_path(name: &str) -> Result<(), AppError> {
    if name.contains(['\\', ':'])
        || name.ends_with(['.', ' '])
        || Path::new(name)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid("备份包含不安全路径"));
    }
    if name != "data/main.sqlite3" && !name.starts_with("media/") {
        return Err(invalid("备份包含未声明类型的文件"));
    }
    if name.split('/').any(|part| {
        part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.split('.').next().is_some_and(|stem| {
                matches!(
                    stem.to_ascii_uppercase().as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
            })
    }) {
        return Err(invalid("备份包含不安全 Windows 路径"));
    }
    Ok(())
}

fn media(conn: &Connection) -> Result<Vec<(String, u64, String)>, AppError> {
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM media m JOIN word_media wm ON wm.media_id=m.id WHERE m.status!='ready' OR m.size<0)", [], |row| row.get::<_, bool>(0))? {
        return Err(invalid("存在尚未就绪或大小无效的引用媒体，请先修复后备份"));
    }
    Ok(conn.prepare("SELECT DISTINCT m.relative_path,m.size,m.sha256 FROM media m JOIN word_media wm ON wm.media_id=m.id ORDER BY m.relative_path")?.query_map([],|r|Ok((r.get(0)?,r.get::<_,i64>(1)? as u64,r.get(2)?)))?.collect::<Result<_,_>>()?)
}

/// @description 在串行写队列中建立含引用媒体的一致完整备份。
/// @param conn 正式库连接；root 数据根；destination 不存在的目标文件。
/// @throws 返回快照、缺媒体、磁盘空间或归档错误，原目标不变。
pub(super) fn create(
    conn: &Connection,
    root: &Path,
    destination: &Path,
) -> Result<Manifest, AppError> {
    controlled(
        conn,
        root,
        destination,
        &super::jobs::Control::default(),
        None,
    )
}

pub(super) fn controlled(
    conn: &Connection,
    root: &Path,
    destination: &Path,
    control: &super::jobs::Control,
    expected: Option<&super::output::Fingerprint>,
) -> Result<Manifest, AppError> {
    control.check()?;
    if super::output::fingerprint(destination)?.as_ref() != expected {
        return Err(AppError::new("OUTPUT_CHANGED", "目标已改变或未经覆盖确认"));
    }
    let directory = root.join("staging").join(id());
    std::fs::create_dir_all(directory.join("data"))?;
    let snapshot = directory.join("data/main.sqlite3");
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("目标路径无效"))?;
    let temporary = parent.join(format!(".nyako-backup-{}.tmp", id()));
    let result = (|| {
        let page_count = conn.query_row("PRAGMA page_count", [], |r| r.get::<_, u32>(0))? as u64;
        let page_size = conn.query_row("PRAGMA page_size", [], |r| r.get::<_, u32>(0))? as u64;
        let references = media(conn)?;
        let required = references.iter().try_fold(
            page_count.saturating_mul(page_size),
            |sum, (_, size, _)| {
                sum.checked_add(*size)
                    .ok_or_else(|| invalid("备份大小溢出"))
            },
        )?;
        if required > MAX_EXPANDED {
            return Err(invalid("备份展开超过 8GiB"));
        }
        let reservation = required.saturating_mul(3).saturating_add(32 * 1024 * 1024);
        if fs2::available_space(root)? < reservation || fs2::available_space(parent)? < reservation
        {
            return Err(AppError::new(
                "DISK_FULL",
                "空间不足以创建数据库快照和完整备份",
            ));
        }
        let mut target = Connection::open(&snapshot)?;
        let backup = Backup::new(conn, &mut target)?;
        loop {
            control.check()?;
            match backup.step(128)? {
                rusqlite::backup::StepResult::Done => break,
                rusqlite::backup::StepResult::More => {}
                rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                _ => return Err(invalid("数据库快照返回未知状态")),
            }
        }
        drop(backup);
        let mut paths = vec![("data/main.sqlite3".to_string(), snapshot.clone())];
        for (relative, size, expected_hash) in media(&target)? {
            let name = format!("media/{relative}");
            safe_path(&name)?;
            let path = root.join(&name);
            if file_hash(&path)? != (size, expected_hash) {
                return Err(invalid("媒体内容或长度与数据库不一致，未生成备份"));
            }
            paths.push((name, path));
        }
        let manifest = Manifest {
            format: "nyako-backup".into(),
            version: 1,
            schema_version: migrations::MIGRATIONS.len(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: now()?,
            words: target.query_row("SELECT count(*) FROM words", [], |r| r.get(0))?,
            events: target.query_row("SELECT count(*) FROM review_events", [], |r| r.get(0))?,
            media: paths.len() - 1,
            files: paths
                .iter()
                .map(|(name, path)| {
                    let (size, sha256) = file_hash(path)?;
                    Ok(Entry {
                        path: name.clone(),
                        size,
                        sha256,
                    })
                })
                .collect::<Result<_, AppError>>()?,
        };
        target.close().map_err(|(_, error)| AppError::from(error))?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("manifest.json", options)
            .map_err(json_error)?;
        zip.write_all(encode(&manifest)?.as_bytes())?;
        for (name, path) in &paths {
            control.check()?;
            zip.start_file(name, options).map_err(json_error)?;
            copy_controlled(&mut File::open(path)?, &mut zip, control)?;
            control
                .processed
                .fetch_add(1, std::sync::atomic::Ordering::Release);
        }
        zip.finish().map_err(json_error)?.sync_all()?;
        validate_controlled(&temporary, &directory.join("verify"), control)?;
        control.publish(|| {
            super::output::publish(&temporary, destination, expected)?;
            Ok(())
        })?;
        Ok(manifest)
    })();
    let cleanup_file = if temporary.exists() {
        std::fs::remove_file(&temporary)
    } else {
        Ok(())
    };
    let cleanup_directory = std::fs::remove_dir_all(&directory);
    if result.is_ok() {
        cleanup_file?;
        cleanup_directory?;
    }
    result
}

/// @description 将完整备份限额解包并验证到独立目录。
/// @param source 备份文件；directory 新的隔离暂存目录。
/// @throws 路径、版本、长度、散列、数据库或媒体关联无效时拒绝恢复。
#[cfg(test)]
pub(super) fn validate(source: &Path, directory: &Path) -> Result<Manifest, AppError> {
    validate_controlled(source, directory, &super::jobs::Control::default())
}

pub(super) fn validate_controlled(
    source: &Path,
    directory: &Path,
    control: &super::jobs::Control,
) -> Result<Manifest, AppError> {
    control.check()?;
    std::fs::create_dir_all(directory)?;
    let mut zip = ZipArchive::new(File::open(source)?).map_err(json_error)?;
    if zip.len() > 100_000 {
        return Err(invalid("备份条目超过 10 万"));
    }
    let manifest: Manifest = {
        let mut input = zip.by_name("manifest.json").map_err(json_error)?;
        if input.size() > 16 * 1024 * 1024
            || input.size() > input.compressed_size().saturating_mul(100)
            || input
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid("备份清单体积、压缩比或类型无效"));
        }
        let mut bytes = Vec::new();
        input
            .by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(invalid("备份清单超过 16MiB"));
        }
        serde_json::from_value(strict_json(&bytes)?).map_err(json_error)?
    };
    if manifest.format != "nyako-backup"
        || manifest.version != 1
        || manifest.schema_version > migrations::MIGRATIONS.len()
    {
        return Err(invalid("备份格式或数据库版本不兼容"));
    }
    if manifest.files.len() + 1 != zip.len() {
        return Err(invalid("归档文件数量与清单不一致"));
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for entry in &manifest.files {
        safe_path(&entry.path)?;
        if !names.insert(entry.path.to_lowercase()) {
            return Err(invalid("备份含重复规范化路径"));
        }
        total = total
            .checked_add(entry.size)
            .ok_or_else(|| invalid("展开大小溢出"))?;
        if total > MAX_EXPANDED {
            return Err(invalid("备份展开超过 8GiB"));
        }
    }
    if fs2::available_space(directory)? < total.saturating_add(16 * 1024 * 1024) {
        return Err(AppError::new("DISK_FULL", "没有足够空间校验备份"));
    }
    let mut seen = HashSet::new();
    for index in 0..zip.len() {
        control.check()?;
        control
            .processed
            .store(index, std::sync::atomic::Ordering::Release);
        let mut input = zip.by_index(index).map_err(json_error)?;
        let name = input.name().map_err(json_error)?.into_owned();
        if !seen.insert(name.clone()) {
            return Err(invalid("归档含重复文件"));
        }
        if name == "manifest.json" {
            continue;
        }
        let entry = manifest
            .files
            .iter()
            .find(|entry| entry.path == name)
            .ok_or_else(|| invalid("归档含清单以外文件"))?;
        if input.is_dir()
            || input
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || !matches!(
                input.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(invalid("归档包含不允许的文件类型或压缩方法"));
        }
        if input.size() != entry.size || input.size() > input.compressed_size().saturating_mul(100)
        {
            return Err(invalid("备份长度或压缩比例超限"));
        }
        let path = directory.join(&name);
        std::fs::create_dir_all(path.parent().ok_or_else(|| invalid("归档路径无效"))?)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let copied = copy_controlled(
            &mut input.by_ref().take(entry.size + 1),
            &mut output,
            control,
        )?;
        output.sync_all()?;
        if copied != entry.size || file_hash(&path)? != (entry.size, entry.sha256.clone()) {
            return Err(invalid("备份内容散列不符"));
        }
    }
    let database = directory.join("data/main.sqlite3");
    if !database.is_file() {
        return Err(invalid("备份缺少数据库"));
    }
    let original =
        Connection::open_with_flags(&database, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if original.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?
        != manifest.schema_version as i64
    {
        return Err(invalid("清单与数据库版本不一致"));
    }
    drop(original);
    let conn = migrations::open_database(&database, &directory.join("migration-backups"))?;
    let references = media(&conn)?;
    if conn.query_row("SELECT count(*) FROM words", [], |row| row.get::<_, i64>(0))?
        != manifest.words
        || conn.query_row("SELECT count(*) FROM review_events", [], |row| {
            row.get::<_, i64>(0)
        })? != manifest.events
        || references.len() != manifest.media
        || manifest.files.len() != manifest.media + 1
    {
        return Err(invalid("清单词条、学习记录或媒体统计与数据库不一致"));
    }
    for (relative, size, sha) in references {
        let name = format!("media/{relative}");
        safe_path(&name)?;
        if file_hash(&directory.join(name))? != (size, sha) {
            return Err(invalid("备份媒体引用不完整"));
        }
    }
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    conn.close().map_err(|(_, error)| AppError::from(error))?;
    let seal = directory.join("validated-files.json");
    let entries = manifest
        .files
        .iter()
        .map(|entry| {
            let (size, sha256) = file_hash(&directory.join(&entry.path))?;
            Ok(Entry {
                path: entry.path.clone(),
                size,
                sha256,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    std::fs::write(seal, encode(&entries)?)?;
    Ok(manifest)
}

/// @description 在恢复切换前复核校验后的数据库和媒体未被修改。
/// @param directory 已完成验证的恢复候选目录。
/// @throws 文件缺失、长度或散列变化时拒绝安排恢复。
pub(super) fn verify_candidate(directory: &Path) -> Result<(), AppError> {
    let bytes = std::fs::read(directory.join("validated-files.json"))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("候选清单超限"));
    }
    let entries: Vec<Entry> = serde_json::from_slice(&bytes).map_err(json_error)?;
    if !entries
        .iter()
        .any(|entry| entry.path == "data/main.sqlite3")
    {
        return Err(invalid("候选清单缺少数据库"));
    }
    for entry in entries {
        safe_path(&entry.path)?;
        if file_hash(&directory.join(entry.path))? != (entry.size, entry.sha256) {
            return Err(invalid("校验后恢复候选发生变化，请重新选择备份"));
        }
    }
    Ok(())
}
