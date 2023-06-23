mod models;

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::exit,
};

use actix_plus_static_files::{build_hashmap_from_included_dir, include_dir, Dir, ResourceFiles};
use actix_web::{
    delete, get,
    middleware::Logger,
    post, put,
    web::{self, Data},
    App, HttpResponse, HttpServer, Responder,
};

use actix_multipart::Multipart;
use crossbeam::channel::{unbounded, Receiver, Sender};
use futures::{StreamExt, TryStreamExt};
use log::{error, info};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use swift_det_lib::DetectConfig;

const STACK: usize = 16 * 1024 * 1024; // 16Mb

const DIR: Dir = include_dir!("./front/dist");

struct AppState {
    db: SqlitePool,
    tx: Sender<Task>,
}

#[get("/api/detections/{id}")]
async fn get_detection(path: web::Path<uuid::Uuid>, data: Data<AppState>) -> impl Responder {
    let id = path.into_inner().to_string();

    if let Ok(d) = sqlx::query_as!(
        models::Detection,
        "SELECT * from detections WHERE id = ?",
        id
    )
    .fetch_one(&data.db)
    .await
    {
        HttpResponse::Ok().json(d)
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/api/detections/{id}/boxes")]
async fn get_detection_boxes(path: web::Path<uuid::Uuid>) -> impl Responder {
    let id = path.into_inner().to_string();

    let path = Path::new("./detections").join(id).join("boxes.jpg");

    if path.exists() {
        HttpResponse::Ok()
            .content_type("image/jpeg")
            .body(std::fs::read(path).unwrap())
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/api/detections/{id}/boxes.dzi")]
async fn get_detection_boxes_dzi(path: web::Path<uuid::Uuid>) -> impl Responder {
    let id = path.into_inner().to_string();

    let path = Path::new("./detections").join(id).join("boxes.dzi");

    if path.exists() {
        HttpResponse::Ok()
            .content_type("application/xml")
            .body(std::fs::read(path).unwrap())
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/api/detections/{id}/boxes_files/{a}/{b}")]
async fn get_detection_boxes_tile(path: web::Path<(uuid::Uuid, String, String)>) -> impl Responder {
    let inner = path.into_inner();

    let path = Path::new("./detections")
        .join(inner.0.to_string())
        .join("boxes_files")
        .join(inner.1)
        .join(inner.2);

    dbg!(&path);

    if path.exists() {
        HttpResponse::Ok()
            .content_type("image/jpeg")
            .body(std::fs::read(path).unwrap())
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/api/detections/{id}/origin")]
async fn get_detection_origin(path: web::Path<uuid::Uuid>) -> impl Responder {
    let id = path.into_inner().to_string();

    let path = Path::new("./detections").join(id).join("origin.jpg");

    if path.exists() {
        HttpResponse::Ok()
            .content_type("image/jpeg")
            .body(std::fs::read(path).unwrap())
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/api/detections")]
async fn get_detections(data: Data<AppState>) -> impl Responder {
    let detections = sqlx::query_as!(
        models::Detection,
        "SELECT * from detections ORDER by modified_at"
    )
    .fetch_all(&data.db)
    .await
    .unwrap();

    HttpResponse::Ok().json(detections)
}

static DEFAULT_CONFIG: DetectConfig = DetectConfig {
    batch_size: 1,
    window_size: (1000, 1000),
    overlap: 20,
    input_size: (800, 800),
    heatmap_size: (200, 200),
    mean: [1.785167, 1.533696, 1.380282],
    std: [1.667162, 1.44502, 1.320071],
    tile_max_num: 50,
    model_path: String::new(),
};

async fn save_image(mut payload: Multipart, path: &PathBuf) -> std::io::Result<()> {
    while let Ok(Some(mut field)) = payload.try_next().await {
        // let content_type = field.content_disposition();

        let mut f = std::fs::File::create(&path)?;

        // Field in turn is stream of *Bytes* object
        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            f = f.write_all(&data).map(|_| f)?
        }
    }

    Ok(())
}

#[post("/api/detections")]
async fn create_detection(payload: Multipart, data: Data<AppState>) -> impl Responder {
    let id = uuid::Uuid::new_v4().to_string();

    let base = Path::new("./detections").join(&id);
    std::fs::create_dir_all(&base).unwrap();

    let impath = base.join("origin.jpg");
    save_image(payload, &impath).await.unwrap();

    let params = models::Params::default();
    let detection = models::Detection {
        id,
        params: serde_json::to_string(&params).unwrap(),
        modified_at: chrono::Local::now().naive_local(),
        num: None,
        remark: None,
        progress: None,
    };

    sqlx::query("INSERT INTO detections (id, params, modified_at) VALUES (?, ?, ?)")
        .bind(detection.id.clone())
        .bind(detection.params.clone())
        .bind(detection.modified_at)
        .execute(&data.db)
        .await
        .unwrap();

    let mut config = DEFAULT_CONFIG.clone();
    config.model_path = "./model.onnx".to_string();
    config.window_size = (params.window_size, params.window_size);

    data.tx
        .send(Task {
            id: detection.id.clone(),
            config,
            image: impath,
            threshold: params.threshold,
        })
        .unwrap();

    HttpResponse::Ok().json(detection)
}

#[put("/api/detections/{id}/params")]
async fn update_detection_params(
    path: web::Path<uuid::Uuid>,
    data: Data<AppState>,
    params: web::Json<models::Params>,
) -> impl Responder {
    let id = path.into_inner().to_string();

    if let Ok(mut old) = sqlx::query_as!(
        models::Detection,
        "SELECT * from detections WHERE id = ?",
        id
    )
    .fetch_one(&data.db)
    .await
    {
        let params_string = serde_json::to_string(&params).unwrap();
        sqlx::query("UPDATE detections SET params = ? WHERE id = ?")
            .bind(&params_string)
            .bind(&id)
            .execute(&data.db)
            .await
            .unwrap();

        let old_params: models::Params = serde_json::from_str(&old.params).unwrap();
        if old_params.window_size == params.window_size {
            if old_params.threshold == params.threshold {
                // Nothing changed
                return HttpResponse::Ok().json(old);
            }
            // Only redraw boxes
            let num = draw_boxes(&id, params.threshold).unwrap();
            sqlx::query("UPDATE detections SET num = ? WHERE id = ?")
                .bind(num)
                .bind(id)
                .execute(&data.db)
                .await
                .unwrap();

            old.params = params_string;
            old.num = Some(num);
            HttpResponse::Ok().json(old)
        } else {
            let mut config = DEFAULT_CONFIG.clone();
            config.model_path = "./model.onnx".to_string();
            config.window_size = (params.window_size, params.window_size);
            sqlx::query("UPDATE detections SET progress = null, num = null WHERE id = ?")
                .bind(&id)
                .execute(&data.db)
                .await
                .unwrap();

            data.tx
                .send(Task {
                    id: id.clone(),
                    config,
                    image: Path::new("./detections").join(id).join("origin.jpg"),
                    threshold: params.threshold,
                })
                .unwrap();
            HttpResponse::Accepted().finish()
        }
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[put("/api/detections/{id}/remark")]
async fn update_detection_remark(
    path: web::Path<uuid::Uuid>,
    data: Data<AppState>,
    remark: web::Json<String>,
) -> impl Responder {
    let id = path.into_inner().to_string();

    if let Ok(mut old) = sqlx::query_as!(
        models::Detection,
        "SELECT * from detections WHERE id = ?",
        id
    )
    .fetch_one(&data.db)
    .await
    {
        sqlx::query("UPDATE detections SET remark = ? WHERE id = ?")
            .bind(&remark.0)
            .bind(&id)
            .execute(&data.db)
            .await
            .unwrap();

        old.remark = Some(remark.0);
        HttpResponse::Ok().json(old)
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[delete("/api/detections/{id}")]
async fn delete_detection(path: web::Path<uuid::Uuid>, data: Data<AppState>) -> impl Responder {
    let id = path.into_inner().to_string();

    if let Ok(_) = sqlx::query("DELETE FROM detections WHERE id = ?")
        .bind(id.clone())
        .execute(&data.db)
        .await
    {
        // Delete dir
        std::fs::remove_dir_all(Path::new("./detections").join(&id)).unwrap();
        HttpResponse::Ok().json(id)
    } else {
        HttpResponse::NotFound().body("Not found")
    }
}

#[get("/editor/{id}")]
async fn editor() -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(DIR.get_file("index.html").unwrap().contents())
}

const WORKERS: u8 = 2;

struct Task {
    id: String,
    config: DetectConfig,
    image: PathBuf,
    threshold: f32,
}

fn draw_boxes(task_id: &str, threshold: f32) -> std::io::Result<i64> {
    let base = Path::new("./detections").join(&task_id);

    let boxes_json = std::fs::File::open(base.join("boxes.json"))?;
    let boxes: Vec<swift_det_lib::BBox> = serde_json::from_reader(boxes_json).unwrap();
    let mut img = image::open(base.join("origin.jpg")).unwrap();

    // Draw rects
    let mut num = 0;
    for b in boxes {
        if b.score < threshold {
            continue;
        }
        let w = b.x_max - b.x_min;
        let h = b.y_max - b.y_min;

        imageproc::drawing::draw_hollow_rect_mut(
            &mut img,
            imageproc::rect::Rect::at(b.x_min, b.y_min).of_size(w as u32, h as u32),
            image::Rgba([0, 255, 0, 255]),
        );
        num += 1;
    }

    img.save(base.join("boxes.jpg")).unwrap();

    // Create dzi
    dzi::TileCreator::new_from_image_path(base.join("boxes.jpg").as_path(), 254, 1)
        .unwrap()
        .create_tiles()
        .unwrap();

    Ok(num)
}

enum WorkerEvent {
    ProgressUpdated { progress: i64, id: String },
    Finished { num: i64, id: String },
}

fn worker(i: u8, rx: Receiver<Task>, tx: Sender<WorkerEvent>) {
    let env = swift_det_lib::make_env().unwrap();
    info!("Worker {}: Started", i);
    for task in rx {
        info!("Worker {}: Begin working on {}", i, &task.id);
        match swift_det_lib::detect(
            task.image.to_str().unwrap(),
            task.config,
            &env,
            |current, total| {
                info!("Worker {}: Progress {} / {}", i, current, total);
                tx.send(WorkerEvent::ProgressUpdated {
                    progress: (*current as f64 / *total as f64 * 100.0) as i64,
                    id: task.id.clone(),
                })
                .unwrap();
            },
            false,
        ) {
            Ok(boxes) => {
                info!("Worker {}: Finished {}", i, &task.id);
                // Write boxes
                let base = Path::new("./detections").join(&task.id);
                let boxes_json = std::fs::File::create(base.join("boxes.json")).unwrap();
                serde_json::to_writer(boxes_json, &boxes).unwrap();
                let num = draw_boxes(&task.id, task.threshold).unwrap();

                tx.send(WorkerEvent::Finished {
                    num,
                    id: task.id.clone(),
                })
                .unwrap();
            }
            Err(err) => {
                error!("Worker {}: Error on {}: {}", i, &task.id, err);
            }
        }
        info!("Worker {}: Idle", i)
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    pretty_env_logger::init();

    let pool = match SqlitePoolOptions::new()
        .connect(std::env!("DATABASE_URL"))
        .await
    {
        Ok(pool) => pool,
        Err(_) => {
            eprintln!("Failed to connect to ./data.db!");
            exit(1);
        }
    };

    // Init
    sqlx::query("CREATE TABLE IF NOT EXISTS detections (id TEXT PRIMARY KEY NOT NULL, params TEXT NOT NULL, modified_at DATETIME NOT NULL, progress INTEGER, num INTEGER, remark TEXT)")
    .execute(&pool).await.unwrap();

    let (tx, rx) = unbounded();
    let (prog_tx, prog_rx) = unbounded();

    for i in 0..WORKERS {
        let rx_ = rx.clone();
        let prog_tx_ = prog_tx.clone();
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn(move || worker(i, rx_, prog_tx_))
            .unwrap();
    }

    ctrlc::set_handler(|| {
        info!("Bye-bye");
        exit(0);
    })
    .unwrap();

    let pool_ = pool.clone();
    actix_rt::spawn(async move {
        for event in prog_rx {
            match event {
                WorkerEvent::Finished { num, id } => {
                    sqlx::query("UPDATE detections SET num = ? WHERE id = ?")
                        .bind(num)
                        .bind(id)
                        .execute(&pool_)
                        .await
                        .ok();
                }
                WorkerEvent::ProgressUpdated { progress, id } => {
                    sqlx::query("UPDATE detections SET progress = ? WHERE id = ?")
                        .bind(progress)
                        .bind(id)
                        .execute(&pool_)
                        .await
                        .ok();
                }
            }
        }
    });

    HttpServer::new(move || {
        let map = build_hashmap_from_included_dir(&DIR);
        App::new()
            .app_data(Data::new(AppState {
                db: pool.clone(),
                tx: tx.clone(),
            }))
            .service(get_detection)
            .service(get_detections)
            .service(get_detection_boxes)
            .service(get_detection_boxes_dzi)
            .service(get_detection_boxes_tile)
            .service(get_detection_origin)
            .service(update_detection_params)
            .service(update_detection_remark)
            .service(create_detection)
            .service(delete_detection)
            .service(editor)
            .service(ResourceFiles::new("/", map))
            .wrap(Logger::default())
    })
    .bind(std::env::var("BIND").unwrap_or("127.0.0.1:8000".to_string()))?
    .run()
    .await
}

#[test]
fn test_det() {
    dotenvy::dotenv().ok();
    pretty_env_logger::init();

    std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(|| {
            let impath = "./t.jpg";
            let mut config = DEFAULT_CONFIG.clone();
            config.model_path = "./model.onnx".to_string();
            config.window_size = (1200, 1200);

            let env = swift_det_lib::make_env().unwrap();
            swift_det_lib::detect(
                impath,
                config,
                &env,
                |current, total| {
                    println!("Progress {} / {}", current, total);
                },
                false,
            )
            .unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}
