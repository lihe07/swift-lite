use crate::db::{self, Db};
use crate::error::AppError;
use crate::master::TaskTx;
use crate::params::Params;
use crate::task::PredictionTask;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone)]
pub struct AppState {
    pub pool: Db,
    pub tx: TaskTx,
}

pub fn router(state: AppState) -> Router {
    // Routes carry the explicit `/api` prefix rather than using `nest`, because
    // axum's `nest("/api", ..)` serves `/api` but 404s the trailing-slash `/api/`,
    // whereas Sanic's blueprint serves both. Also note axum's router cannot host
    // both `/detections/:id/params` (static) and `/detections/:id/:im` (param) at
    // the same position, so the third segment is a single `:seg` param dispatched
    // by method + name inside the handlers.
    Router::new()
        .route("/api", get(hello))
        .route("/api/", get(hello))
        .route("/api/april-fools", post(april_fools))
        .route("/api/detections", post(new_detection).get(list_detections))
        .route("/api/detections/:id", get(get_detection).delete(delete_detection))
        .route("/api/detections/:id/:seg", get(get_segment).put(put_segment))
        .route("/api/workers", get(get_workers))
        .with_state(state)
}

async fn hello() -> impl IntoResponse {
    Json(json!({"message": "Hello World"}))
}

async fn april_fools(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    let c = s.pool.get().await?;
    c.execute("INSERT INTO april_fools (created_at) VALUES (NOW())", &[])
        .await?;
    let row = c
        .query_one("SELECT COUNT(*) AS count FROM april_fools", &[])
        .await?;
    let count: i64 = row.get("count");
    Ok(Json(json!({ "count": count })))
}

/// Fetch one detection as the API JSON object (params parsed, optional queue field).
async fn fetch_detection_json(pool: &Db, id: &str) -> Result<Option<Value>, AppError> {
    let c = pool.get().await?;
    let rows = c
        .query("SELECT * FROM detections WHERE id = $1", &[&id])
        .await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let mut obj = db::detection_row_to_json(row);
    if obj["status"] == json!("queue") {
        obj["queue"] = json!(1);
    }
    Ok(Some(obj))
}

async fn get_detection(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    match fetch_detection_json(&s.pool, &id).await? {
        Some(obj) => Ok(Json(obj)),
        None => Err(AppError::NotFound),
    }
}

async fn new_detection(
    State(s): State<AppState>,
    mut mp: Multipart,
) -> Result<Json<Value>, AppError> {
    // find the "file" field
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
    {
        if field.name() == Some("file") {
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Internal(e.to_string()))?
                    .to_vec(),
            );
            break;
        }
    }
    let Some(bytes) = file_bytes else {
        return Err(AppError::BadRequest("No Image".into()));
    };

    let id = uuid::Uuid::new_v4().to_string();
    let base = std::path::PathBuf::from("./detections").join(&id);
    std::fs::create_dir_all(&base).map_err(|e| AppError::Internal(e.to_string()))?;

    // decode + re-encode to jpg (cv2.imdecode/imwrite equivalent)
    let img = image::load_from_memory(&bytes).map_err(|e| AppError::Internal(e.to_string()))?;
    img.to_rgb8()
        .save(base.join("origin.jpg"))
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let params = Params::default();
    let params_str = serde_json::to_string(&params).unwrap();
    let now = db::now();
    {
        let c = s.pool.get().await?;
        c.execute(
            "INSERT INTO detections (id, params, modified_at, created_at, remark, status) VALUES ($1,$2,$3,$4,$5,$6)",
            &[&id, &params_str, &now, &now, &"", &"queue"],
        )
        .await?;
    }
    let _ = s.tx.try_send(id.clone());

    match fetch_detection_json(&s.pool, &id).await? {
        Some(obj) => Ok(Json(obj)),
        None => Err(AppError::NotFound),
    }
}

/// GET /detections/:id/:seg — serve an image. seg in {origin,boxes,windows}.
async fn get_segment(Path((id, seg)): Path<(String, String)>) -> Result<Response, AppError> {
    let file = match seg.as_str() {
        "origin" => "origin.jpg",
        "boxes" => "origin.boxes.jpg",
        "windows" => "origin.windows.jpg",
        _ => return Err(AppError::NotFound),
    };
    let path = std::path::PathBuf::from("./detections").join(&id).join(file);
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg".to_string()),
            (header::CACHE_CONTROL, "max-age=2629746".to_string()),
        ],
        bytes,
    )
        .into_response())
}

/// PUT /detections/:id/:seg — dispatch params/remark by segment name.
async fn put_segment(
    State(s): State<AppState>,
    Path((id, seg)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    match seg.as_str() {
        "params" => put_params(s, id, body).await,
        "remark" => put_remark(s, id, body).await,
        _ => Err(AppError::NotFound),
    }
}

async fn put_params(s: AppState, id: String, body: Value) -> Result<Json<Value>, AppError> {
    // Presence + type check, matching Python "Missing {k}" / tiling bool.
    for k in ["tiling", "window_size", "overlap", "threshold", "iou"] {
        if body.get(k).is_none() {
            return Err(AppError::BadRequest(format!("Missing {k}")));
        }
    }
    if !body["tiling"].is_boolean() {
        return Err(AppError::BadRequest("tiling should be bool".into()));
    }
    let new_params: Params =
        serde_json::from_value(body.clone()).map_err(|e| AppError::BadRequest(e.to_string()))?;
    new_params.validate().map_err(AppError::BadRequest)?;

    // load existing row
    let (status, old_params) = {
        let c = s.pool.get().await?;
        let rows = c
            .query("SELECT status, params FROM detections WHERE id = $1", &[&id])
            .await?;
        let Some(row) = rows.first() else {
            return Err(AppError::NotFound);
        };
        let status: Option<String> = row.try_get("status").ok().flatten();
        let old_params_str: String = row.get("params");
        let old_params: Params = serde_json::from_str(&old_params_str)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        (status, old_params)
    };

    if status.as_deref() != Some("done") {
        // no change; return current detection
        return current_detection(&s, &id).await;
    }

    if old_params == new_params {
        // unchanged -> return row (parsed)
        return current_detection(&s, &id).await;
    }

    let new_params_str = serde_json::to_string(&new_params).unwrap();
    db::update_detection(&s.pool, &id, None, Some("queue"), None, Some(&new_params_str))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let geometry_same = old_params.window_size == new_params.window_size
        && old_params.overlap == new_params.overlap
        && old_params.tiling == new_params.tiling;

    if geometry_same {
        // only threshold/iou changed -> recompute inline, no worker
        let task = PredictionTask {
            id: id.clone(),
            params: new_params,
        };
        task.nms_only(&s.pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        return current_detection(&s, &id).await;
    }

    let _ = s.tx.try_send(id.clone());
    current_detection(&s, &id).await
}

async fn put_remark(s: AppState, id: String, body: Value) -> Result<Json<Value>, AppError> {
    let remark = body.get("remark").and_then(|v| v.as_str()).unwrap_or("");
    {
        let c = s.pool.get().await?;
        let rows = c
            .query("SELECT id FROM detections WHERE id = $1", &[&id])
            .await?;
        if rows.is_empty() {
            return Err(AppError::NotFound);
        }
    }
    db::update_detection(&s.pool, &id, None, None, Some(remark), None)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    current_detection(&s, &id).await
}

/// Return the current detection JSON or 404.
async fn current_detection(s: &AppState, id: &str) -> Result<Json<Value>, AppError> {
    match fetch_detection_json(&s.pool, id).await? {
        Some(obj) => Ok(Json(obj)),
        None => Err(AppError::NotFound),
    }
}

async fn delete_detection(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let base = std::path::PathBuf::from("./detections").join(&id);
    if base.exists() {
        let _ = std::fs::remove_dir_all(&base);
    }
    let c = s.pool.get().await?;
    c.execute("DELETE FROM detections WHERE id = $1", &[&id])
        .await?;
    Ok(Json(json!({ "id": id })))
}

async fn get_workers(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    let c = s.pool.get().await?;
    let cutoff = crate::master::online_cutoff(db::now());
    let rows = c
        .query(
            "SELECT id, name, connected_at, avg_det_time, last_ping, tasks_done FROM workers WHERE last_ping >= $1 ORDER BY last_ping DESC",
            &[&cutoff],
        )
        .await?;
    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, String>("id"),
                "name": r.try_get::<_, Option<String>>("name").ok().flatten(),
                "connected_at": r.try_get::<_, Option<i32>>("connected_at").ok().flatten(),
                "avg_det_time": r.try_get::<_, Option<f64>>("avg_det_time").ok().flatten(),
                "last_ping": r.try_get::<_, Option<i32>>("last_ping").ok().flatten(),
                "tasks_done": r.try_get::<_, Option<i32>>("tasks_done").ok().flatten(),
            })
        })
        .collect();
    Ok(Json(json!({ "data": data })))
}

async fn list_detections(
    State(s): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let size: i64 = q.get("size").and_then(|v| v.parse().ok()).unwrap_or(20);
    let page: i64 = q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    let sortby = q.get("sortby").map(|s| s.as_str()).unwrap_or("modified_at");
    let sort = q.get("sort").map(|s| s.as_str()).unwrap_or("desc");

    if !(1..=100).contains(&size) {
        return Err(AppError::BadRequest("Size should be within [1, 100]".into()));
    }
    if page < 1 {
        return Err(AppError::BadRequest("Page should be greater than 0".into()));
    }
    if !["modified_at", "num", "status", "created_at"].contains(&sortby) {
        return Err(AppError::BadRequest("Invalid sortby".into()));
    }
    if !["asc", "desc"].contains(&sort) {
        return Err(AppError::BadRequest("Invalid sort".into()));
    }

    let c = s.pool.get().await?;
    // sortby/sort are whitelisted above, safe to interpolate
    let sql = format!(
        "SELECT id,num,modified_at,created_at,remark,status FROM detections ORDER BY {sortby} {sort} NULLS LAST LIMIT $1 OFFSET $2"
    );
    let rows = c.query(&sql, &[&size, &((page - 1) * size)]).await?;
    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<_, String>("id"),
                "num": r.try_get::<_, Option<i32>>("num").ok().flatten(),
                "modified_at": r.try_get::<_, Option<i32>>("modified_at").ok().flatten(),
                "created_at": r.try_get::<_, Option<i32>>("created_at").ok().flatten(),
                "remark": r.try_get::<_, Option<String>>("remark").ok().flatten(),
                "status": r.try_get::<_, Option<String>>("status").ok().flatten(),
            })
        })
        .collect();
    let total: i64 = c
        .query_one("SELECT COUNT(*) AS count FROM detections", &[])
        .await?
        .get("count");
    Ok(Json(json!({ "total": total, "data": data })))
}

#[cfg(test)]
mod route_tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    // hello has no DB dependency; mirror the production prefix layout.
    fn hello_router() -> Router {
        Router::new()
            .route("/api", get(hello))
            .route("/api/", get(hello))
    }

    async fn status_of(uri: &str) -> (u16, serde_json::Value) {
        let resp = hello_router()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value =
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        (status, v)
    }

    #[tokio::test]
    async fn hello_route_works_with_and_without_trailing_slash() {
        let (s1, v1) = status_of("/api").await;
        assert_eq!(s1, 200);
        assert_eq!(v1["message"], "Hello World");
        let (s2, v2) = status_of("/api/").await;
        assert_eq!(s2, 200);
        assert_eq!(v2["message"], "Hello World");
    }

    // Build the production router with a lazy (never-connected) pool. This proves the
    // full route table — including the merged `/detections/:id/:seg` — registers without
    // a matchit conflict panic, and exercises the no-DB code paths.
    fn real_router() -> Router {
        let pool = crate::db::make_pool("postgresql://swift:swift@localhost:5432/swift").unwrap();
        let (tx, _rx) = async_channel::unbounded::<String>();
        router(AppState { pool, tx })
    }

    async fn real_status(method: &str, uri: &str) -> u16 {
        real_router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    #[tokio::test]
    async fn production_router_builds_and_serves_hello() {
        // construction alone would panic on a route conflict
        assert_eq!(real_status("GET", "/api/").await, 200);
        assert_eq!(real_status("GET", "/api").await, 200);
    }

    #[tokio::test]
    async fn missing_image_is_404_without_db() {
        // get_segment checks the file before touching the DB, so this needs no live DB.
        assert_eq!(
            real_status("GET", "/api/detections/does-not-exist/origin").await,
            404
        );
        // unknown segment also 404s before any DB access
        assert_eq!(
            real_status("GET", "/api/detections/does-not-exist/bogus").await,
            404
        );
    }
}
