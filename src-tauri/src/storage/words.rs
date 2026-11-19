use super::{
    libraries::{conflict, missing, now},
    models::*,
    settings::transaction,
    validation,
};
use crate::{database::normalize, error::AppError};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn get(conn: &Connection, id: &str) -> Result<WordDetail, AppError> {
    validation::id(id)?;
    let mut detail = conn.query_row("SELECT w.kanji,w.kana,w.jlpt,w.note,w.revision,w.created_at,w.updated_at,c.state,c.due_at,coalesce(c.suspended,0),c.buried_until,w.meaning,w.pitch,w.examples_jp,w.examples_cn FROM words w LEFT JOIN cards c ON c.word_id=w.id AND c.card_type='recognition' WHERE w.id=?1", [id], |r| Ok(WordDetail {
        content: WordInput { id:id.into(),kanji:r.get(0)?,kana:r.get(1)?,jlpt:r.get(2)?,note:r.get(3)?,meaning:r.get(11)?,pitch:r.get(12)?,examples_jp:r.get(13)?,examples_cn:r.get(14)?,pos:vec![],library_ids:vec![] },
        revision:r.get(4)?,created_at:r.get(5)?,updated_at:r.get(6)?,card_state:r.get(7)?,due_at:r.get(8)?,suspended:r.get(9)?,buried_until:r.get(10)?,
    })).optional()?.ok_or_else(missing)?;
    detail.content.pos = strings(
        conn,
        "SELECT pos_code FROM word_pos WHERE word_id=?1 ORDER BY pos_code",
        id,
    )?;
    detail.content.library_ids = strings(
        conn,
        "SELECT library_id FROM library_words WHERE word_id=?1 ORDER BY library_id",
        id,
    )?;
    Ok(detail)
}

fn strings(conn: &Connection, sql: &str, id: &str) -> Result<Vec<String>, AppError> {
    Ok(conn
        .prepare(sql)?
        .query_map([id], |r| r.get(0))?
        .collect::<Result<_, _>>()?)
}

fn sync_memberships(conn: &Connection, value: &WordInput) -> Result<(), AppError> {
    conn.execute("DELETE FROM word_pos WHERE word_id=?1", [&value.id])?;
    for pos in &value.pos {
        conn.execute("INSERT INTO word_pos VALUES(?1,?2)", params![value.id, pos])?;
    }
    for old in strings(
        conn,
        "SELECT library_id FROM library_words WHERE word_id=?1",
        &value.id,
    )? {
        if !value.library_ids.contains(&old) {
            conn.execute(
                "DELETE FROM library_words WHERE word_id=?1 AND library_id=?2",
                params![value.id, old],
            )?;
        }
    }
    for id in &value.library_ids {
        conn.execute(
            "INSERT OR IGNORE INTO library_words VALUES(?1,?2,?3)",
            params![id, value.id, now()?],
        )?;
    }
    Ok(())
}

pub(super) fn save(
    conn: &mut Connection,
    input: WordInput,
    expected: Option<i64>,
) -> Result<WordDetail, AppError> {
    transaction(conn, |tx| save_in_transaction(tx, input, expected))
}

pub(super) fn save_in_transaction(
    tx: &rusqlite::Transaction<'_>,
    input: WordInput,
    expected: Option<i64>,
) -> Result<WordDetail, AppError> {
    let value = validation::word(input)?;
    let time = now()?;
    let search = std::iter::once(&value.kanji)
        .chain(std::iter::once(&value.kana))
        .chain(std::iter::once(&value.meaning))
        .chain(std::iter::once(&value.note))
        .map(|s| normalize(s))
        .collect::<Vec<_>>()
        .join("\n");
    match expected {
            None => {
                tx.execute("INSERT INTO words(id,kanji,kana,jlpt,note,search_text,created_at,updated_at,meaning,pitch,examples_jp,examples_cn) VALUES(?1,?2,?3,?4,?5,?6,?7,?7,?8,?9,?10,?11)",params![value.id,value.kanji,value.kana,value.jlpt,value.note,search,time,value.meaning,value.pitch,value.examples_jp,value.examples_cn])?;
                tx.execute("INSERT INTO cards(id,word_id,card_type,state,scheduler_version,parameter_set_id,eligible_since) VALUES(?1,?2,'recognition','new','6.6.2','fsrs6-default',?3)",params![uuid::Uuid::new_v4().to_string(),value.id,time])?;
            }
            Some(revision) => {
                if tx.execute("UPDATE words SET kanji=?1,kana=?2,jlpt=?3,note=?4,search_text=?5,updated_at=?6,revision=revision+1 ,meaning=?9,pitch=?10,examples_jp=?11,examples_cn=?12 WHERE id=?7 AND revision=?8",params![value.kanji,value.kana,value.jlpt,value.note,search,time,value.id,revision,value.meaning,value.pitch,value.examples_jp,value.examples_cn])? != 1 { return Err(conflict()); }
            }
        }
    sync_memberships(tx, &value)?;
    get(tx, &value.id)
}

pub(super) fn impact(
    conn: &Connection,
    selected: &[SelectedWord],
) -> Result<DeleteImpact, AppError> {
    check_selected(conn, selected)?;
    let mut result = DeleteImpact {
        memberships: 0,
        cards: 0,
        events: 0,
    };
    for word in selected {
        result.memberships += conn.query_row(
            "SELECT count(*) FROM library_words WHERE word_id=?1",
            [&word.id],
            |r| r.get::<_, i64>(0),
        )?;
        result.cards += conn.query_row(
            "SELECT count(*) FROM cards WHERE word_id=?1",
            [&word.id],
            |r| r.get::<_, i64>(0),
        )?;
        result.events+=conn.query_row("SELECT count(*) FROM review_events WHERE card_id IN (SELECT id FROM cards WHERE word_id=?1)",[&word.id],|r|r.get::<_,i64>(0))?;
    }
    Ok(result)
}

pub(super) fn check_selected(conn: &Connection, selected: &[SelectedWord]) -> Result<(), AppError> {
    if selected.is_empty() {
        return Err(AppError::validation("请先选择词条"));
    }
    validation::unique(selected.iter().map(|s| s.id.as_str()), 500)?;
    let conflicts = selected
        .iter()
        .filter_map(|s| {
            match conn
                .query_row("SELECT revision FROM words WHERE id=?1", [&s.id], |r| {
                    r.get::<_, i64>(0)
                })
                .optional()
            {
                Ok(Some(revision)) if revision == s.revision => None,
                Ok(_) => Some(Ok(s.id.clone())),
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !conflicts.is_empty() {
        return Err(AppError::new(
            "REVISION_CONFLICT",
            format!("选择中有已更新或删除的词条：{}", conflicts.join(", ")),
        ));
    }
    Ok(())
}

pub(super) fn batch(
    conn: &mut Connection,
    selected: Vec<SelectedWord>,
    action: &str,
    target: Option<String>,
) -> Result<usize, AppError> {
    if !["add_library", "remove_library", "delete"].contains(&action) {
        return Err(AppError::validation("未知批量操作"));
    }
    if action != "delete" {
        validation::id(
            target
                .as_deref()
                .ok_or_else(|| AppError::validation("请选择目标"))?,
        )?;
    }
    transaction(conn, |tx| {
        check_selected(tx, &selected)?;
        if action != "delete"
            && !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM libraries WHERE id=?1)",
                [&target],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(missing());
        }
        for word in &selected {
            match action {
                "delete" => {
                    tx.execute("DELETE FROM words WHERE id=?1", [&word.id])?;
                }
                "add_library" => {
                    tx.execute(
                        "INSERT OR IGNORE INTO library_words VALUES(?1,?2,?3)",
                        params![target, word.id, now()?],
                    )?;
                }
                "remove_library" => {
                    tx.execute(
                        "DELETE FROM library_words WHERE library_id=?1 AND word_id=?2",
                        params![target, word.id],
                    )?;
                }
                _ => unreachable!(),
            }
            if action != "delete" {
                tx.execute(
                    "UPDATE words SET revision=revision+1,updated_at=?1 WHERE id=?2",
                    params![now()?, word.id],
                )?;
            }
        }
        Ok(selected.len())
    })
}
