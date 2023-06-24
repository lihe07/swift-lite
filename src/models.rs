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

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PaginationAndSort {
    pub page: u32,
    pub page_size: u32,
    pub sort_key: String,
    pub asc: bool,
}

impl Default for PaginationAndSort {
    fn default() -> Self {
        PaginationAndSort {
            page: 1,
            page_size: 10,
            sort_key: "modified_at".to_string(),
            asc: false,
        }
    }
}

impl PaginationAndSort {
    pub fn validate(&self) -> Option<&str> {
        if self.page_size < 3 || self.page_size > 20 {
            return Some("page_size should fit in [3, 20]");
        }

        if self.sort_key != "modified_at" && self.sort_key != "num" {
            return Some("sort_key can only be modified_at or num");
        }

        None
    }

    pub fn sort(&self) -> String {
        self.sort_key.to_string() + " " + if self.asc { "ASC" } else { "DESC" }
    }

    pub fn limit(&self) -> u32 {
        self.page_size
    }

    pub fn offset(&self) -> u32 {
        (self.page - 1) * self.page_size
    }
}
