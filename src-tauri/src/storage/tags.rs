use super::{
    libraries::{conflict, now},
    models::Tag,
    settings::transaction,
    validation,
};
use crate::error::AppError;
use rusqlite::{params, Connection};

pub(super) fn list(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    Ok(conn
        .prepare("SELECT id,name FROM tags ORDER BY name,id")?
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

pub(super) fn save(
    conn: &mut Connection,
    id: String,
    name: String,
    expected: Option<String>,
) -> Result<Vec<Tag>, AppError> {
    validation::id(&id)?;
    let name = validation::text(&name, 1, 1000)?;
    transaction(conn, |tx| {
        match expected {
            None => {
                tx.execute("INSERT INTO tags VALUES(?1,?2)", params![id, name])?;
            }
            Some(old) => {
                if tx.execute(
                    "UPDATE tags SET name=?1 WHERE id=?2 AND name=?3",
                    params![name, id, old],
                )? != 1
                {
                    return Err(conflict());
                }
                tx.execute("UPDATE words SET revision=revision+1,updated_at=?1 WHERE id IN (SELECT word_id FROM word_tags WHERE tag_id=?2)",params![now()?,id])?;
            }
        }
        list(tx)
    })
}

pub(super) fn delete(conn: &mut Connection, id: String, expected: String) -> Result<(), AppError> {
    validation::id(&id)?;
    transaction(conn, |tx| {
        tx.execute("UPDATE words SET revision=revision+1,updated_at=?1 WHERE id IN (SELECT word_id FROM word_tags WHERE tag_id=?2)",params![now()?,id])?;
        if tx.execute(
            "DELETE FROM tags WHERE id=?1 AND name=?2",
            params![id, expected],
        )? != 1
        {
            return Err(conflict());
        }
        Ok(())
    })
}
