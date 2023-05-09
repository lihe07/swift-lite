import cv2
from pydantic import BaseModel
import numpy as np
import onnxruntime
import os
import pyvips
import shutil

sess = onnxruntime.InferenceSession("skyseg.onnx")

def run_inference(onnx_session, image, threshold = 0.1):
    x = cv2.resize(image, (320, 320))
    x = np.array(x, dtype=np.float32)
    mean = [0.485, 0.456, 0.406]
    std = [0.229, 0.224, 0.225]
    x = (x / 255 - mean) / std
    x = x.transpose(2, 0, 1)
    x = x.reshape(-1, 3, 320, 320).astype('float32')

    # Inference
    input_name = onnx_session.get_inputs()[0].name
    output_name = onnx_session.get_outputs()[0].name
    result = onnx_session.run([output_name], {input_name: x})

    # Post process
    result = np.array(result).squeeze()
    min_value = np.min(result)
    max_value = np.max(result)
    result = (result - min_value) / (max_value - min_value)
    result = cv2.threshold(result, (1 - threshold), 1, cv2.THRESH_BINARY)[1]
    result = result.astype(np.uint8)
    return result



class DetParameters(BaseModel):
    is_pure_sky: bool = False
    sky_threshold: float = 0.1
    sky_erode: int = 150
    point_threshold: float = 0.2


def create_dzi(dir): 
    im = os.path.join(dir, "merged.jpg")
    dzi = os.path.join(dir, "merged")
    files = os.path.join(dir, "merged_files")

    # If files exists, remove it
    if os.path.exists(files):
        shutil.rmtree(files)

    image = pyvips.Image.new_from_file(im, access="sequential")
    image.dzsave(dzi, suffix=".jpg[Q=90]", tile_size=256, overlap=0)


def detect(params: DetParameters, dir: str):
    im = cv2.imread(os.path.join(dir, "origin.jpg"))
    im = cv2.cvtColor(im, cv2.COLOR_BGR2RGB)

    if params.is_pure_sky:
        result = np.ones((im.shape[0], im.shape[1]), dtype=np.uint8)
    else:
        result = run_inference(sess, im, params.sky_threshold)

        # 开闭
        kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (5, 5))
        result = cv2.morphologyEx(result, cv2.MORPH_CLOSE, kernel)
        # Stretch the result
        result = cv2.resize(result, (im.shape[1], im.shape[0]))

        if params.sky_erode > 0:
            kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (params.sky_erode, params.sky_erode))
            result = cv2.erode(result, kernel)

    color_mask = np.zeros_like(im)
    # Fill Green on Non-SKY region
    color_mask[result == 0] = (2, 209, 224)

    cv2.imwrite(os.path.join(dir, "mask.png"), color_mask)


    # Apply BlackHat
    gray = cv2.cvtColor(im, cv2.COLOR_RGB2GRAY)
    kernel = cv2.getStructuringElement(cv2.MORPH_RECT, (5, 5))
    filtered = cv2.morphologyEx(gray, cv2.MORPH_BLACKHAT, kernel)
    # Convert to binary
    # filtered = cv2.threshold(filtered, 255 * params.point_threshold, 255, cv2.THRESH_BINARY)[1]
    filtered = cv2.adaptiveThreshold(filtered, 255, cv2.ADAPTIVE_THRESH_GAUSSIAN_C, cv2.THRESH_BINARY, 11, 2)

    # Clean non-sky pixels
    filtered = cv2.bitwise_and(filtered, result)

    # Find Connected Components
    nb_components, output, stats, centroids = cv2.connectedComponentsWithStats(
        filtered, connectivity=8)

    with open(os.path.join(dir, "num"), mode="w") as f:
        f.write(str(nb_components))

    # Draw the connected components
    # sizes = stats[1:, -1]

    rect = np.zeros((output.shape[0], output.shape[1], 3), dtype=np.uint8)

    boxes = open(os.path.join(dir, "boxes"), "w")
    for i in range(0, nb_components - 1):
        x = stats[i + 1, cv2.CC_STAT_LEFT]
        y = stats[i + 1, cv2.CC_STAT_TOP]
        w = stats[i + 1, cv2.CC_STAT_WIDTH]
        h = stats[i + 1, cv2.CC_STAT_HEIGHT]

        cv2.rectangle(rect, (x, y), (x + w, y + h), (255, 34, 145), 2)
        if nb_components < 5000:
            boxes.write(str(x) + " " + str(y) + " " + str(w) + " " + str(h) + "\n")

    filtered = cv2.addWeighted(rect, 1, im, 1, 0)

    filtered = cv2.cvtColor(filtered, cv2.COLOR_RGB2BGR)

    cv2.imwrite(os.path.join(dir, "boxes.png"), filtered)

    cv2.imwrite(os.path.join(dir, "merged.jpg"), cv2.addWeighted(color_mask, 1, filtered, 1, 0))
    create_dzi(dir)

    return nb_components