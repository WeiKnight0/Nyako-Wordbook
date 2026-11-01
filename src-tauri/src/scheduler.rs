use crate::error::AppError;
use fsrs::{MemoryState, FSRS};
use serde::Serialize;

/// @description 四级反馈对应的初始记忆状态与到期时间。
/// @param 不适用，数据类型。
/// @throws 不适用。
#[derive(Debug, Serialize, PartialEq)]
pub struct RatingPreview {
    pub rating: u8,
    pub stability_days: f32,
    pub difficulty: f32,
    pub interval_days: f32,
    pub due_at_ms: i64,
}

/// @description 按固定保持率生成初次学习的四级反馈预览。
/// @param now_ms 当前时间戳，用于计算到期时间。
/// @throws 返回 FSRS 计算失败的业务错误。
pub(super) fn preview(now_ms: i64) -> Result<Vec<RatingPreview>, AppError> {
    let states = FSRS::default()
        .next_states(None, 0.9, 0)
        .map_err(|_| AppError::validation("FSRS 无法计算初始状态"))?;
    Ok([states.again, states.hard, states.good, states.easy]
        .into_iter()
        .enumerate()
        .map(|(i, state)| {
            let delay = match i {
                0 => 60_000,
                1 => 300_000,
                2 => 600_000,
                _ => (state.interval.clamp(1.0, 36500.0).round() as i64) * 86_400_000,
            };
            RatingPreview {
                rating: i as u8 + 1,
                stability_days: state.memory.stability,
                difficulty: state.memory.difficulty,
                interval_days: state.interval,
                due_at_ms: now_ms + delay,
            }
        })
        .collect())
}

/// @description 检查同日复习生成的记忆状态有效性。
/// @param 无。
/// @throws 返回 FSRS 计算失败的业务错误。
pub(super) fn verify_same_day() -> Result<bool, AppError> {
    let fsrs = FSRS::default();
    let current = MemoryState {
        stability: 2.3065,
        difficulty: 2.118104,
    };
    let next = fsrs
        .next_states(Some(current), 0.9, 0)
        .map_err(|_| AppError::validation("FSRS 无法计算同日复习"))?;
    Ok([next.again, next.hard, next.good, next.easy]
        .iter()
        .all(|s| {
            s.memory.stability.is_finite()
                && s.memory.stability > 0.0
                && (1.0..=10.0).contains(&s.memory.difficulty)
                && s.interval.is_finite()
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_initial_vector_and_determinism() {
        let actual = preview(1_793_404_800_000).unwrap();
        let expected = [
            (0.212, 6.4133),
            (1.2931, 5.1121707),
            (2.3065, 2.118104),
            (8.2956, 1.0),
        ];
        for (value, (stability, difficulty)) in actual.iter().zip(expected) {
            assert!((value.stability_days - stability).abs() < 0.00001);
            assert!((value.difficulty - difficulty).abs() < 0.00001);
            assert!((value.interval_days - stability).abs() < 0.00001);
        }
        assert_eq!(actual, preview(1_793_404_800_000).unwrap());
        assert_eq!(actual[0].due_at_ms, 1_793_404_860_000);
        assert!(verify_same_day().unwrap());
    }
}
