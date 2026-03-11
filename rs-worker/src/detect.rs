use image::DynamicImage;
use itertools::iproduct;
use ndarray::{s, Array, Array3, Array4, Axis};
use ort::{
    execution_providers::CPUExecutionProvider,
    session::{builder::GraphOptimizationLevel, Session, SessionOutputs},
    value::TensorRef,
};
use thiserror::Error;

use crate::nms::{nms_center_opt, BBOX};

// --- CONSTANTS ---
const BATCH_SIZE: usize = 32;
const INPUT_SIZE: (u32, u32) = (640, 640);
static MODEL: &[u8] = include_bytes!("../best.onnx");

// --- ERROR HANDLING ---
#[derive(Error, Debug)]
pub enum DetectionError {
    #[error("ONNX Runtime error: {0}")]
    Ort(#[from] ort::Error),
    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),
    #[error("NDArray shape error: {0}")]
    Shape(#[from] ndarray::ShapeError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// --- RESULT STRUCTURES ---
/// Holds the results of a detection run for clarity.
pub struct DetectionResult {
    /// Final bounding boxes after NMS.
    pub bboxes: Vec<BBOX>,
    /// Top-left coordinates of each sliding window.
    pub window_top_lefts: Vec<(i32, i32)>,
    /// Dimensions of the sliding windows (height, width).
    pub window_dimensions: (u32, u32),
    /// Number of windows along each axis (y, x).
    pub window_counts: (usize, usize),
}

// --- DETECTOR ---
pub struct Detector {
    session: Session,
}

impl Detector {
    /// Creates a new Detector by loading the ONNX model.
    pub fn new() -> Result<Self, DetectionError> {
        let session = Session::builder()
            .map_err(ort::Error::from)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(ort::Error::from)?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .map_err(ort::Error::from)?
            .commit_from_memory(MODEL)?;

        Ok(Self { session })
    }

    /// Recreates torch.nn.functional.interpolate using the `image` crate.
    /// Resizes a batch of CHW images to the target input size.
    fn rust_interpolate(
        &self,
        batch: &Array4<f32>,
        input_size: (u32, u32),
    ) -> Result<Array4<f32>, DetectionError> {
        let (batch_size, channels, _, _) = batch.dim();
        let (target_h, target_w) = input_size;

        // Create an empty array for the resized batch
        let mut resized_batch =
            Array4::<f32>::zeros((batch_size, channels, target_h as usize, target_w as usize));

        for (b, c) in iproduct!(0..batch_size, 0..channels) {
            let img_plane = batch.slice(s![b, c, .., ..]);
            let (height, width) = (img_plane.dim().0 as u32, img_plane.dim().1 as u32);

            // Create an image buffer from the ndarray view
            // Note: The data is already in 0-1 f32 format.
            let buffer = image::ImageBuffer::<image::Luma<f32>, _>::from_raw(
                width,
                height,
                img_plane.to_slice().unwrap(),
            )
            .unwrap();

            // Resize using bilinear interpolation (Triangle filter)
            let resized_buffer = image::imageops::resize(
                &buffer,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            );

            // Convert the resized buffer back to an ndarray
            let resized_array = Array::from_shape_vec(
                (target_h as usize, target_w as usize),
                resized_buffer.into_raw(),
            )?;

            // Place the resized plane into the output batch
            resized_batch
                .slice_mut(s![b, c, .., ..])
                .assign(&resized_array);
        }

        Ok(resized_batch)
    }

    /// Performs object detection on an image using a sliding window approach.
    pub fn detect(
        &mut self,
        img: &DynamicImage,
        window_size_ratio: f32,
        overlap_ratio: f32,
        score_threshold: f32,
        iou_threshold: f32,
    ) -> Result<DetectionResult, DetectionError> {
        // --- Image Preprocessing ---
        let rgb_img = img.to_rgb32f();
        let (h, w) = (rgb_img.height(), rgb_img.width());

        // Convert image to CHW ndarray and normalize
        let img_tensor = Array3::from_shape_vec((h as usize, w as usize, 3), rgb_img.into_raw())?
            .permuted_axes([2, 0, 1])
            .to_owned();
        // The `to_rgb32f` already normalizes to [0, 1], so no division by 255 is needed.

        // --- Sliding Window Calculation ---
        let max_dim = h.max(w) as f32;
        let window_size = window_size_ratio * max_dim;
        let overlap = overlap_ratio * window_size;

        let x_num = ((w as f32 - overlap) / (window_size - overlap))
            .ceil()
            .max(1.0) as usize;
        let y_num = ((h as f32 - overlap) / (window_size - overlap))
            .ceil()
            .max(1.0) as usize;

        let window_width = ((w as f32 - overlap) / x_num as f32 + overlap) as u32;
        let window_height = ((h as f32 - overlap) / y_num as f32 + overlap) as u32;

        let scale = (
            window_width as f32 / INPUT_SIZE.1 as f32,
            window_height as f32 / INPUT_SIZE.0 as f32,
        );

        // --- Window Generation ---
        let mut windows = Vec::new();
        let mut windows_lt = Vec::new(); // Top-left coordinates

        for i in 0..x_num {
            for j in 0..y_num {
                let x = (i as f32 * (window_width as f32 - overlap)) as i32;
                let y = (j as f32 * (window_height as f32 - overlap)) as i32;

                let window = img_tensor.slice(s![
                    ..,
                    y as usize..(y + window_height as i32) as usize,
                    x as usize..(x + window_width as i32) as usize,
                ]);
                windows.push(window.to_owned());
                windows_lt.push((x, y));
            }
        }

        // --- Batch Inference ---
        let mut all_boxes: Vec<BBOX> = Vec::new();
        for (i, batch_windows) in windows.chunks(BATCH_SIZE).enumerate() {
            // Stack windows into a single batch tensor
            let batch_views: Vec<_> = batch_windows.iter().map(|w| w.view()).collect();
            let batch_array = ndarray::stack(Axis(0), &batch_views)?;

            // Interpolate the batch to the model's input size
            let interpolated_batch = self.rust_interpolate(&batch_array, INPUT_SIZE)?;

            // Run inference
            let model_input =
                ort::inputs!["input" => TensorRef::from_array_view(&interpolated_batch)?];

            let outputs: SessionOutputs = self.session.run(model_input)?;

            // Extract the output tensor: Shape (Batch, 102000, 6)
            let result_tensor = outputs["output"].try_extract_array::<f32>()?;

            // --- Post-processing ---
            for j in 0..result_tensor.shape()[0] {
                // Iterate through each image's results in the batch
                let single_result = result_tensor.slice(s![j, .., ..]);
                let window_offset = windows_lt[i * BATCH_SIZE + j];

                for row in single_result.rows() {
                    let score = row[4];
                    if score > score_threshold {
                        let mut box_data: BBOX = (
                            row[0], // x_center
                            row[1], // y_center
                            row[2], // width
                            row[3], // height
                            score,  // confidence
                        );

                        // Scale and translate box to original image coordinates
                        box_data.0 = box_data.0 * scale.0 + window_offset.0 as f32;
                        box_data.1 = box_data.1 * scale.1 + window_offset.1 as f32;
                        box_data.2 *= scale.0;
                        box_data.3 *= scale.1;

                        all_boxes.push(box_data);
                    }
                }
            }
        }

        // --- Non-Maximum Suppression ---
        // Your custom NMS function is called here. It works directly on the
        // (x_center, y_center, width, height, confidence) format.
        let final_boxes = nms_center_opt(all_boxes, iou_threshold);

        Ok(DetectionResult {
            bboxes: final_boxes,
            window_top_lefts: windows_lt,
            window_dimensions: (window_height, window_width),
            window_counts: (y_num, x_num),
        })
    }
}
