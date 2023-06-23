use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Deserialize, Serialize, FromRow)]
pub struct Detection {
    pub id: String,
    pub params: String,
    pub modified_at: chrono::NaiveDateTime,
    pub progress: Option<i64>,
    pub num: Option<i64>,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Params {
    pub threshold: f32,
    pub window_size: usize,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            threshold: 0.4,
            window_size: 800,
        }
    }
}
