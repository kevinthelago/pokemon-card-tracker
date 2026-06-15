use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cursor-based pagination parameters extracted from query strings.
///
/// Clients pass `?cursor=<opaque>&limit=50`.
/// The next page cursor is returned in the response envelope.
#[derive(Debug, Deserialize)]
pub struct CursorPage {
    pub cursor: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

impl CursorPage {
    pub fn clamped_limit(&self) -> i64 {
        self.limit.clamp(1, 200)
    }
}

/// Standard paginated response envelope.
#[derive(Debug, Serialize)]
pub struct PageResponse<T: Serialize> {
    pub data: Vec<T>,
    pub next_cursor: Option<Uuid>,
    pub has_more: bool,
}

impl<T: Serialize> PageResponse<T> {
    pub fn new(mut data: Vec<T>, limit: usize, next_cursor: impl Fn(&T) -> Uuid) -> Self {
        let has_more = data.len() > limit;
        let cursor = if has_more {
            data.truncate(limit);
            data.last().map(next_cursor)
        } else {
            None
        };
        Self {
            data,
            next_cursor: cursor,
            has_more,
        }
    }
}
