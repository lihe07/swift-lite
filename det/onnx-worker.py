import cv2
import math
import torch
from torchvision.ops import nms
import numpy as np
import socket
import struct
import requests
import time
import json
import onnxruntime as ort

BATCH_SIZE = 32

INPUT_SIZE = (640, 640)

MASTER = "back.bwrrc.org.cn:12345"

MODEL = "./best.onnx"

model = ort.InferenceSession(MODEL, providers=["CPUExecutionProvider"])


def numpy_interpolate(batch, input_size):
    """
    Recreate torch.nn.functional.interpolate using OpenCV
    """
    batch_size, channels, height, width = batch.shape
    resized_batch = np.zeros(
        (batch_size, channels, input_size[0], input_size[1]), dtype=batch.dtype
    )

    for i in range(batch_size):
        for j in range(channels):
            # OpenCV's resize expects (width, height) order for the size parameter
            resized_batch[i, j] = cv2.resize(
                batch[i, j],
                (input_size[1], input_size[0]),  # (width, height)
                interpolation=cv2.INTER_LINEAR,  # Bilinear interpolation
            )

    return resized_batch


def detect(
    img,
    window_size: float,
    overlap: float,
    score_threshold=0.3,
    iou_threshold=0.5,
):
    img = cv2.cvtColor(img, cv2.COLOR_BGR2RGB)
    img = np.transpose(img, (2, 0, 1)).astype(np.float32) / 255.0
    h = img.shape[1]
    w = img.shape[2]

    window_size = window_size * max(h, w)
    overlap = overlap * window_size

    x_num = math.ceil((w - overlap) / (window_size - overlap))
    y_num = math.ceil((h - overlap) / (window_size - overlap))

    x_num = max(1, x_num)
    y_num = max(1, y_num)

    window_width = int((w - overlap) / x_num + overlap)
    window_height = int((h - overlap) / y_num + overlap)

    # scale = (INPUT_SIZE[0] / window_width, INPUT_SIZE[1] / window_height)
    scale = (window_width / INPUT_SIZE[0], window_height / INPUT_SIZE[1])

    # divide windows
    windows = []
    windows_lt = []

    for i in range(x_num):
        for j in range(y_num):
            x = i * (window_width - overlap)
            x = int(x)
            y = j * (window_height - overlap)
            y = int(y)
            windows.append(img[:, y : y + window_height, x : x + window_width])
            windows_lt.append((x, y))

    # predict, batch
    results = np.zeros((0, 6), dtype=np.float32)
    for i in range(0, len(windows), BATCH_SIZE):
        batch = windows[i : i + BATCH_SIZE]
        # batch = torch.stack(batch)
        batch = np.array(batch, dtype=np.float32)
        print(batch.shape)

        # interpolate
        # batch = torch.nn.functional.interpolate(
        #     batch, size=INPUT_SIZE, mode="bilinear", align_corners=False
        # )
        batch = numpy_interpolate(batch, INPUT_SIZE)

        model_input = {
            "input": batch,
        }
        result = model.run(output_names=["output"], input_feed=model_input)
        # (B, 102000, 6) 6: x, y, w, h, score, class
        result = result[0]
        print("result shape:", result.shape)

        for j in range(len(result)):
            boxes = result[j][result[j][:, 4] > score_threshold]

            boxes[:, 0] = boxes[:, 0] * scale[0] + windows_lt[i + j][0]
            boxes[:, 1] = boxes[:, 1] * scale[1] + windows_lt[i + j][1]
            boxes[:, 2] = boxes[:, 2] * scale[0]
            boxes[:, 3] = boxes[:, 3] * scale[1]
            # results = torch.cat((results, boxes), 0)
            results = np.vstack((results, boxes))

    results = results[:, :5]
    # x y w h -> x1, y1, x2, y2
    results[:, 0] = results[:, 0] - results[:, 2] / 2
    results[:, 1] = results[:, 1] - results[:, 3] / 2
    results[:, 2] = results[:, 0] + results[:, 2]
    results[:, 3] = results[:, 1] + results[:, 3]

    # temporarily convert to torch tensor for NMS
    keep = nms(torch.tensor(results[:, :4]), torch.tensor(results[:, 4]), iou_threshold)
    # keep = nms(results[:, :4], results[:, 4], iou_threshold)
    results = results[keep]

    return results, windows_lt, (window_height, window_width), (y_num, x_num)


if __name__ == "__main__":
    boxes, windows_lt, window_size, window_num = detect(
        cv2.imread("/home/lihe07/Documents/HIC-Yolo/big.jpg"), 0.3, 0.1
    )

    im = cv2.imread("/home/lihe07/Documents/HIC-Yolo/big.jpg")
    for box in boxes:
        x1, y1, x2, y2 = map(int, box[:4])
        cv2.rectangle(im, (x1, y1), (x2, y2), (0, 255, 0), 2)

    cv2.imwrite("./result.jpg", im)

    for x, y in windows_lt:
        x, y = int(x), int(y)
        h, w = int(window_size[0]), int(window_size[1])
        cv2.rectangle(im, (x, y), (x + w, y + h), (255, 0, 0), 2)

    cv2.imwrite("./windows.jpg", im)
    print(len(boxes))
