//! Result statistics (WCA averages), shared by the cubebox UI and the tests.

use super::record::DNF_TIME_MS;
use super::session::Penalty;

/// Result in centiseconds with the penalty applied; `None` for DNF.
pub fn centis(duration_ms: u32, penalty: Penalty) -> Option<u32> {
    match penalty {
        Penalty::Dnf => None,
        Penalty::PlusTwo => Some(duration_ms.saturating_add(2000) / 10),
        Penalty::None => Some(duration_ms / 10),
    }
}

/// WCA average of `n` results (best and worst removed).
///
/// `results` are centiseconds, newest first; `None` is a DNF and sorts last.
/// Returns `None` when there are not enough results or when the trimmed window
/// still contains a DNF (which makes the whole average a DNF).
pub fn average(results: &[Option<u32>], n: usize) -> Option<i64> {
    if n < 3 || results.len() < n {
        return None;
    }
    let mut window: Vec<Option<u32>> = results.iter().take(n).copied().collect();
    window.sort_by(|a, b| match (a, b) {
        (Some(x), Some(y)) => x.cmp(y),
        (None, None) => core::cmp::Ordering::Equal,
        (None, Some(_)) => core::cmp::Ordering::Greater,
        (Some(_), None) => core::cmp::Ordering::Less,
    });
    let trimmed = &window[1..window.len() - 1];
    if trimmed.iter().any(Option::is_none) {
        return None;
    }
    let sum: u64 = trimmed.iter().map(|v| v.unwrap_or(0) as u64).sum();
    Some((sum as f64 / trimmed.len() as f64).round() as i64)
}

/// Best non-DNF result (centiseconds).
pub fn best(results: &[Option<u32>]) -> Option<u32> {
    results.iter().flatten().copied().min()
}

/// Duration used for a DNF result, re-exported so callers need one import.
pub const DNF_MS: u32 = DNF_TIME_MS;
