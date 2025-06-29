// --- NETWORK HELPERS ---

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::Instant,
};

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("ONNX Runtime error: {0}")]
    Ort(#[from] ort::Error),
    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),
    #[error("NDArray shape error: {0}")]
    Shape(#[from] ndarray::ShapeError),
    #[error("Network I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP request error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Invalid command from master")]
    InvalidCommand,
}

/// The JSON query received from the master.
#[derive(Deserialize, Debug)]
struct PredictQuery {
    window_size: f32,
    overlap: f32,
    threshold: f32,
    iou: f32,
}

/// The JSON response sent back to the master.
#[derive(Serialize)]
struct PredictionResponse {
    // Note: BBOX is not directly serializable. We convert it to Vec<Vec<f32>>.
    boxes: Vec<Vec<f32>>,
    windows_lt: Vec<(i32, i32)>,
    window_size: (u32, u32),
    window_num: (usize, usize),
    det_time: f64,
    transfer_time: f64,
}

/// Reads from the stream until a null terminator `\0` is found.
async fn read_until_zero(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    loop {
        let byte = stream.read_u8().await?;
        if byte == 0 {
            break;
        }
        buffer.push(byte);
    }
    Ok(buffer)
}

/// Reads a specific number of bytes from the stream.
async fn read_exact_length(stream: &mut TcpStream, length: u32) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0; length as usize];
    stream.read_exact(&mut buffer).await?;
    Ok(buffer)
}

// --- MAIN SERVER LOGIC ---

pub async fn main_loop() -> Result<(), Box<dyn std::error::Error>> {
    let master_addr = goldberg::goldberg_string!("123.57.231.31:12345").to_owned();

    // Connect to the master server
    println!("Connecting to master at {}...", &master_addr);
    let mut stream = TcpStream::connect(master_addr).await?;
    stream.set_nodelay(true)?; // Disable Nagle's algorithm for lower latency
    println!("Connected to master.");

    // Initialize the detector once
    let mut detector = crate::Detector::new()?;

    // Main worker loop
    loop {
        // Read a command from the master, terminated by a null byte
        let command = match read_until_zero(&mut stream).await {
            Ok(cmd) if cmd.is_empty() => {
                // println!("Master closed the connection.");
                break; // Connection closed
            }
            Ok(cmd) => cmd,
            Err(_) => {
                // eprintln!("Failed to read command from master: {e}");
                break;
            }
        };

        match command.as_slice() {
            b"ping" => {
                // println!("Received 'ping', sending 'pong'");
                stream.write_all(b"pong\0").await?;
            }
            b"predict" | b"predict_url" => {
                let t00 = Instant::now();
                let is_url = command.as_slice() == b"predict_url";
                // println!("Received '{}' command.", String::from_utf8_lossy(&command));

                // Read image data (or URL)
                let img_len = stream.read_u32().await?;
                let mut img_data = read_exact_length(&mut stream, img_len).await?;

                if is_url {
                    let url = String::from_utf8(img_data)?;
                    // println!("Downloading image from: {url}");
                    img_data = reqwest::get(&url).await?.bytes().await?.to_vec();
                }

                // Read JSON query
                let query_len = stream.read_u32().await?;
                let query_data = read_exact_length(&mut stream, query_len).await?;

                // Read final null terminator
                if stream.read_u8().await? != 0 {
                    return Err(Box::new(WorkerError::InvalidCommand));
                }

                let transfer_time = t00.elapsed();
                // println!("Data received in {:.3}s.", transfer_time.as_secs_f64());

                // Decode data and run detection
                let t_det = Instant::now();
                let img = image::load_from_memory(&img_data)?;
                let query: PredictQuery = serde_json::from_slice(&query_data)?;

                // println!(
                //     "Image loaded with size {}x{} and query: {:?}",
                //     img.width(),
                //     img.height(),
                //     query
                // );

                println!("Starting detection...");
                let result = detector.detect(
                    &img,
                    query.window_size,
                    query.overlap,
                    query.threshold,
                    query.iou,
                )?;
                let det_time = t_det.elapsed();
                println!(
                    "Detection finished in {:.3}s. Found {} boxes.",
                    det_time.as_secs_f64(),
                    result.bboxes.len()
                );

                // Prepare and send the response
                let response = PredictionResponse {
                    boxes: result
                        .bboxes
                        .iter()
                        // .map(|b| vec![b.0, b.1, b.2, b.3, b.4])
                        // x y w h -> x y x y
                        .map(|b| {
                            vec![
                                b.0 - b.2 * 0.5, // x1
                                b.1 - b.3 * 0.5, // y1
                                b.0 + b.2 * 0.5, // x2
                                b.1 + b.3 * 0.5, // y2
                                b.4,             // confidence
                            ]
                        })
                        .collect(),
                    windows_lt: result.window_top_lefts,
                    window_size: result.window_dimensions,
                    window_num: result.window_counts,
                    det_time: det_time.as_secs_f64(),
                    transfer_time: transfer_time.as_secs_f64(),
                };

                let response_bytes = serde_json::to_vec(&response)?;

                stream.write_u32(response_bytes.len() as u32).await?;
                stream.write_all(&response_bytes).await?;
                stream.write_u8(0).await?; // Final null terminator
                // println!("Response sent.");
            }
            _ => {
                // eprintln!("Received unknown command: {:?}", command);
                break; // Unknown command, close connection
            }
        }
    }

    Ok(())
}
