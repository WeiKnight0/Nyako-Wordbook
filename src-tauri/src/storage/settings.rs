use crate::error::AppError;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

/// @description 在提交前保证词条聚合完整，并原子提交业务修改。
/// @param conn 串行服务持有的数据库连接；action 事务中的业务修改。
/// @throws 返回业务校验或数据库错误，事务自动回滚。
pub(super) fn transaction<T>(
    conn: &mut Connection,
    action: impl FnOnce(&Transaction<'_>) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = action(&tx)?;
    if tx.query_row("SELECT EXISTS(SELECT 1 FROM words w WHERE NOT EXISTS(SELECT 1 FROM senses s WHERE s.word_id=w.id))", [], |r| r.get::<_, bool>(0))? {
        return Err(AppError::validation("每个词条至少需要一个有效义项"));
    }
    tx.commit()?;
    Ok(result)
}

fn check_revision(actual: i64, expected: i64) -> Result<(), AppError> {
    if actual != expected {
        return Err(AppError::new(
            "REVISION_CONFLICT",
            "内容已更新，请重新读取后再保存",
        ));
    }
    Ok(())
}

/// @description 校验允许的设置并以乐观版本控制保存。
/// @param conn 存储连接；key 设置名称；value JSON 内容；expected 读取时的版本。
/// @throws 返回校验、版本冲突或数据库错误。
pub(super) fn save_setting(
    conn: &mut Connection,
    key: &str,
    value: &str,
    expected: Option<i64>,
) -> Result<i64, AppError> {
    let parsed: serde_json::Value =
        serde_json::from_str(value).map_err(|_| AppError::validation("设置内容不是有效 JSON"))?;
    if !match key {
        "daily_new_limit" => parsed.as_u64().is_some_and(|v| v <= 200),
        "day_start_hour" => parsed.as_u64().is_some_and(|v| v <= 6),
        "theme" => matches!(parsed.as_str(), Some("system" | "light" | "dark")),
        _ => false,
    } {
        return Err(AppError::validation("未知设置或设置值超出允许范围"));
    }
    transaction(conn, |tx| {
        let current: Option<i64> = tx
            .query_row("SELECT revision FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        match (current, expected) {
            (Some(actual), Some(expected)) => check_revision(actual, expected)?,
            (None, None) => {}
            _ => {
                return Err(AppError::new(
                    "REVISION_CONFLICT",
                    "设置版本已改变，请重新读取",
                ))
            }
        }
        let revision = current.unwrap_or(0) + 1;
        tx.execute("INSERT INTO settings VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,revision=excluded.revision", rusqlite::params![key, value, revision])?;
        Ok(revision)
    })
}
