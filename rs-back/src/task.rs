use crate::db::{self, Db};
use crate::draw;
use crate::nms::nms;
use crate::params::Params;
use image::RgbImage;
use serde_json::Value;
use std::path::PathBuf;

pub struct PredictionTask {
    pub id: String,
    pub params: Params,
}

fn base_dir(id: &str) -> PathBuf {
    PathBuf::from("./detections").join(id)
}

impl PredictionTask {
    /// Load a task from the DB by id. Returns None if the row is missing.
    pub async fn from_id(pool: &Db, id: &str) -> anyhow::Result<Option<PredictionTask>> {
        let c = pool.get().await?;
        let rows = c
            .query("SELECT id, params FROM detections WHERE id = $1", &[&id])
            .await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let params_str: String = row.get("params");
        let params: Params = serde_json::from_str(&params_str)?;
        Ok(Some(PredictionTask {
            id: row.get("id"),
            params,
        }))
    }

    /// Local origin image path (always a file path, never http).
    pub fn image_url(&self) -> String {
        format!("./detections/{}/origin.jpg", self.id)
    }

    pub async fn set_status(&self, pool: &Db, status: &str) -> anyhow::Result<()> {
        db::update_detection(pool, &self.id, None, Some(status), None, None).await
    }

    /// Reload saved result.json and re-run post-processing (no worker round-trip).
    pub async fn nms_only(&self, pool: &Db) -> anyhow::Result<()> {
        let base = base_dir(&self.id);
        let raw = std::fs::read_to_string(base.join("result.json"))?;
        let result: Value = serde_json::from_str(&raw)?;
        self.done(pool, &result).await
    }

    /// Post-process a worker result. Mirrors back/common.py:PredictionTask.done.
    pub async fn done(&self, pool: &Db, result: &Value) -> anyhow::Result<()> {
        let base = base_dir(&self.id);
        std::fs::create_dir_all(&base)?;

        // 1. write raw result
        std::fs::write(base.join("result.json"), serde_json::to_vec(result)?)?;

        // 2. NMS -> boxes.json
        let raw_boxes: Vec<[f32; 5]> = result["boxes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let v = b.as_array()?;
                        Some([
                            v[0].as_f64()? as f32,
                            v[1].as_f64()? as f32,
                            v[2].as_f64()? as f32,
                            v[3].as_f64()? as f32,
                            v[4].as_f64()? as f32,
                        ])
                    })
                    .collect()
            })
            .unwrap_or_default();
        let boxes = nms(&raw_boxes, self.params.threshold as f32, self.params.iou as f32);
        let boxes_json: Vec<Vec<f32>> = boxes.iter().map(|b| b.to_vec()).collect();
        std::fs::write(base.join("boxes.json"), serde_json::to_vec(&boxes_json)?)?;

        // 3. draw boxes on origin -> origin.boxes.jpg
        let mut img: RgbImage = image::open(base.join("origin.jpg"))?.to_rgb8();
        draw::draw_boxes(&mut img, &boxes);
        img.save(base.join("origin.boxes.jpg"))?;

        // 4. draw windows on the same image -> origin.windows.jpg
        let ws = &result["window_size"];
        let window_h = ws[0].as_f64().unwrap_or(0.0) as i32;
        let window_w = ws[1].as_f64().unwrap_or(0.0) as i32;
        let windows_lt: Vec<(i32, i32)> = result["windows_lt"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let v = p.as_array()?;
                        Some((v[0].as_f64()? as i32, v[1].as_f64()? as i32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        draw::draw_windows(&mut img, &windows_lt, window_h, window_w);
        img.save(base.join("origin.windows.jpg"))?;

        // 5. update DB
        db::update_detection(
            pool,
            &self.id,
            Some(boxes.len() as i32),
            Some("done"),
            None,
            None,
        )
        .await?;
        Ok(())
    }
}
