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
    let mut detail = conn.query_row("SELECT w.kanji,w.kana,w.jlpt,w.note,w.revision,w.created_at,w.updated_at,c.state,c.due_at,coalesce(c.suspended,0),c.buried_until FROM words w LEFT JOIN cards c ON c.word_id=w.id AND c.card_type='recognition' WHERE w.id=?1", [id], |r| Ok(WordDetail {
        content: WordInput { id:id.into(),kanji:r.get(0)?,kana:r.get(1)?,jlpt:r.get(2)?,note:r.get(3)?,senses:vec![],examples:vec![],pitches:vec![],pos:vec![],library_ids:vec![] },
        revision:r.get(4)?,created_at:r.get(5)?,updated_at:r.get(6)?,card_state:r.get(7)?,due_at:r.get(8)?,suspended:r.get(9)?,buried_until:r.get(10)?,
    })).optional()?.ok_or_else(missing)?;
    detail.content.senses = conn
        .prepare("SELECT id,meaning FROM senses WHERE word_id=?1 ORDER BY position,id")?
        .query_map([id], |r| {
            Ok(Sense {
                id: r.get(0)?,
                meaning: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    detail.content.examples = conn
        .prepare("SELECT id,sense_id,jp,cn FROM examples WHERE word_id=?1 ORDER BY position,id")?
        .query_map([id], |r| {
            Ok(Example {
                id: r.get(0)?,
                sense_id: r.get(1)?,
                jp: r.get(2)?,
                cn: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    detail.content.pitches = conn
        .prepare("SELECT id,downstep FROM pitch_accents WHERE word_id=?1 ORDER BY position")?
        .query_map([id], |r| {
            Ok(Pitch {
                id: r.get(0)?,
                downstep: r.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
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

fn owned(conn: &Connection, table: &str, id: &str, word: &str) -> Result<(), AppError> {
    if conn
        .query_row(
            &format!("SELECT word_id FROM {table} WHERE id=?1"),
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .is_some_and(|owner| owner != word)
    {
        return Err(AppError::validation("子记录标识属于其他词条"));
    }
    Ok(())
}

fn sync_children(conn: &Connection, value: &WordInput) -> Result<(), AppError> {
    for sense in &value.senses {
        owned(conn, "senses", &sense.id, &value.id)?;
    }
    for example in &value.examples {
        owned(conn, "examples", &example.id, &value.id)?;
    }
    for pitch in &value.pitches {
        owned(conn, "pitch_accents", &pitch.id, &value.id)?;
    }
    // 临时偏移保留原身份，并避免互换位置时触发唯一约束。
    conn.execute(
        "UPDATE senses SET position=position+1000000 WHERE word_id=?1",
        [&value.id],
    )?;
    conn.execute(
        "UPDATE examples SET position=position+1000000 WHERE word_id=?1",
        [&value.id],
    )?;
    conn.execute(
        "UPDATE pitch_accents SET position=position+1000000 WHERE word_id=?1",
        [&value.id],
    )?;
    for (position, sense) in value.senses.iter().enumerate() {
        conn.execute("INSERT INTO senses VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET position=excluded.position,meaning=excluded.meaning",params![sense.id,value.id,position as i64,sense.meaning])?;
    }
    for (position, example) in value.examples.iter().enumerate() {
        conn.execute("INSERT INTO examples VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET sense_id=excluded.sense_id,position=excluded.position,jp=excluded.jp,cn=excluded.cn",params![example.id,value.id,example.sense_id,position as i64,example.jp,example.cn])?;
    }
    conn.execute(
        "DELETE FROM examples WHERE word_id=?1 AND position>=1000000",
        [&value.id],
    )?;
    conn.execute(
        "DELETE FROM senses WHERE word_id=?1 AND position>=1000000",
        [&value.id],
    )?;
    for old in strings(
        conn,
        "SELECT id FROM pitch_accents WHERE word_id=?1",
        &value.id,
    )? {
        if !value.pitches.iter().any(|p| p.id == old) {
            conn.execute("DELETE FROM pitch_accents WHERE id=?1", [old])?;
        }
    }
    for (position, pitch) in value.pitches.iter().enumerate() {
        conn.execute("INSERT INTO pitch_accents(id,word_id,downstep,position) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET downstep=excluded.downstep,position=excluded.position",params![pitch.id,value.id,pitch.downstep,position as i64])?;
    }
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
        .chain(value.senses.iter().map(|s| &s.meaning))
        .chain(std::iter::once(&value.note))
        .map(|s| normalize(s))
        .collect::<Vec<_>>()
        .join("\n");
    match expected {
            None => {
                tx.execute("INSERT INTO words(id,kanji,kana,jlpt,note,search_text,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![value.id,value.kanji,value.kana,value.jlpt,value.note,search,time])?;
                tx.execute("INSERT INTO cards(id,word_id,card_type,state,scheduler_version,parameter_set_id,eligible_since) VALUES(?1,?2,'recognition','new','6.6.2','fsrs6-default',?3)",params![uuid::Uuid::new_v4().to_string(),value.id,time])?;
            }
            Some(revision) => {
                if tx.execute("UPDATE words SET kanji=?1,kana=?2,jlpt=?3,note=?4,search_text=?5,updated_at=?6,revision=revision+1 WHERE id=?7 AND revision=?8",params![value.kanji,value.kana,value.jlpt,value.note,search,time,value.id,revision])? != 1 { return Err(conflict()); }
            }
        }
    sync_children(tx, &value)?;
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
