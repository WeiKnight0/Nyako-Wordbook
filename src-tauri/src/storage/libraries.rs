use super::{models::Library, settings::transaction, validation};
use crate::error::AppError;
use rusqlite::{params, Connection};

pub(super) fn now() -> Result<i64, AppError> {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AppError::validation("系统时间无效"))?
            .as_millis(),
    )
    .map_err(|_| AppError::validation("系统时间超出支持范围"))
}

pub(super) fn conflict() -> AppError {
    AppError::new(
        "REVISION_CONFLICT",
        "内容已改变，请保留草稿并重新读取最新版本",
    )
}
pub(super) fn missing() -> AppError {
    AppError::new("NOT_FOUND", "内容已删除或不存在")
}

pub(super) fn list(conn: &Connection) -> Result<Vec<Library>, AppError> {
    Ok(conn.prepare("SELECT l.id,l.name,l.description,l.archived,l.revision,(SELECT count(*) FROM library_words m WHERE m.library_id=l.id) FROM libraries l ORDER BY l.archived,l.created_at,l.id")?
        .query_map([], |r| Ok(Library { id:r.get(0)?, name:r.get(1)?, description:r.get(2)?, archived:r.get(3)?, revision:r.get(4)?, word_count:r.get(5)? }))?
        .collect::<Result<Vec<_>, _>>()?)
}

pub(super) fn save(
    conn: &mut Connection,
    library: Library,
    expected: Option<i64>,
) -> Result<Vec<Library>, AppError> {
    validation::id(&library.id)?;
    let name = validation::text(&library.name, 1, 100)?;
    let description = validation::text(&library.description, 0, 10000)?;
    transaction(conn, |tx| {
        let time = now()?;
        match expected {
            None => { tx.execute("INSERT INTO libraries VALUES(?1,?2,?3,?4,1,?5,?5)", params![library.id,name,description,library.archived,time])?; }
            Some(revision) => {
                if tx.execute("UPDATE libraries SET name=?1,description=?2,archived=?3,revision=revision+1,updated_at=?4 WHERE id=?5 AND revision=?6", params![name,description,library.archived,time,library.id,revision])? != 1 { return Err(conflict()); }
            }
        }
        list(tx)
    })
}

pub(super) fn delete(conn: &mut Connection, id: &str, revision: i64) -> Result<(), AppError> {
    validation::id(id)?;
    transaction(conn, |tx| {
        tx.execute("UPDATE words SET revision=revision+1,updated_at=?1 WHERE id IN (SELECT word_id FROM library_words WHERE library_id=?2)", params![now()?,id])?;
        if tx.execute(
            "DELETE FROM libraries WHERE id=?1 AND revision=?2",
            params![id, revision],
        )? != 1
        {
            return Err(conflict());
        }
        Ok(())
    })
}
