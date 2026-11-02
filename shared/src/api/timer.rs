//! `timer` service endpoints (practice records).
//!
//! Verified against UAT:
//! - `timer/time/practice/report`        single record  (`ReportTimerDataReq`)
//! - `timer/time/practice/batch/report`  `{"list":[...]}` (this is the offline
//!   bulk-upload endpoint)
//! - `timer/data/preview/list`           recent records, usable to verify
//! - both POST-only, no token/signature required.

use anyhow::Result;

use super::{ApiClient, ApiReply};
use crate::timer::{batch_json, TimerRecord};

pub const PATH_REPORT: &str = "timer/time/practice/report";
pub const PATH_BATCH_REPORT: &str = "timer/time/practice/batch/report";
pub const PATH_PREVIEW_LIST: &str = "timer/data/preview/list";
pub const PATH_SIMPLE_DATA: &str = "timer/time/practice/simple/data";

/// Aggregates returned after a successful report.
#[derive(Clone, Copy, Debug, Default)]
pub struct TimerAggregate {
    pub recent_mo3: i64,
    pub recent_ao5: i64,
    pub recent_ao12: i64,
    pub recent_ao50: i64,
    pub recent_ao100: i64,
    pub recent_tps: i64,
    pub play_count_all: i64,
    pub play_count_valid: i64,
}

impl ApiClient {
    /// Upload a single practice record.
    pub fn report_one(&self, rec: &TimerRecord) -> Result<ApiReply> {
        let body = rec.to_json();
        self.post_json(PATH_REPORT, &body)
    }

    /// Upload a batch of practice records (the offline sync path).
    pub fn report_batch(&self, recs: &[TimerRecord]) -> Result<ApiReply> {
        let body = batch_json(recs);
        self.post_json(PATH_BATCH_REPORT, &body)
    }

    /// Read back the recent-record list (verification helper).
    pub fn preview_list(&self) -> Result<ApiReply> {
        self.post_json(PATH_PREVIEW_LIST, "{}")
    }

    /// Read back aggregates for the current group.
    pub fn simple_data(&self) -> Result<ApiReply> {
        self.post_json(PATH_SIMPLE_DATA, "{}")
    }
}

/// Pull the aggregate numbers out of a report reply.
pub fn parse_aggregate(reply: &ApiReply) -> Option<TimerAggregate> {
    let data = reply.data()?;
    let get = |k: &str| data.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    Some(TimerAggregate {
        recent_mo3: get("recent_mo3"),
        recent_ao5: get("recent_ao5"),
        recent_ao12: get("recent_ao12"),
        recent_ao50: get("recent_ao50"),
        recent_ao100: get("recent_ao100"),
        recent_tps: get("recent_tps"),
        play_count_all: data
            .get("play_count_alll")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        play_count_valid: get("play_count_valid"),
    })
}
