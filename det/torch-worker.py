import cv2
import torch
import math
from torchvision.ops import nms
import numpy as np
import socket
import struct
import requests
import time
import json

BATCH_SIZE = 32
HAS_CUDA = torch.cuda.is_available()

INPUT_SIZE = (640, 640)

MASTER = "back.bwrrc.org.cn:12345"

MODEL = "./best.torchscript.pt"
CUDA = 0

if HAS_CUDA:
    model = torch.jit.load(MODEL, map_location="cuda:" + str(CUDA))
else:
    model = torch.jit.load(MODEL)


def detect(
    img,
    window_size: float,
    overlap: float,
    score_threshold=0.3,
    iou_threshold=0.5,
):
    img = cv2.cvtColor(img, cv2.COLOR_BGR2RGB)
    img = torch.tensor(img).permute(2, 0, 1).float() / 255.0
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
    results = torch.zeros(0, 6)
    for i in range(0, len(windows), BATCH_SIZE):
        batch = windows[i : i + BATCH_SIZE]
        batch = torch.stack(batch)

        # interpolate
        batch = torch.nn.functional.interpolate(
            batch, size=INPUT_SIZE, mode="bilinear", align_corners=False
        )

        if HAS_CUDA:
            batch = batch.cuda(CUDA)
        result = model(batch)[0]  # (B, 102000, 6) 6: x, y, w, h, score, class
        result = result.cpu()

        for j in range(len(result)):
            boxes = result[j][result[j][:, 4] > score_threshold]

            boxes[:, 0] = boxes[:, 0] * scale[0] + windows_lt[i + j][0]
            boxes[:, 1] = boxes[:, 1] * scale[1] + windows_lt[i + j][1]
            boxes[:, 2] = boxes[:, 2] * scale[0]
            boxes[:, 3] = boxes[:, 3] * scale[1]
            results = torch.cat((results, boxes), 0)

    results = results[:, :5]
    results[:, 0] = results[:, 0] - results[:, 2] / 2
    results[:, 1] = results[:, 1] - results[:, 3] / 2
    results[:, 2] = results[:, 0] + results[:, 2]
    results[:, 3] = results[:, 1] + results[:, 3]

    keep = nms(results[:, :4], results[:, 4], iou_threshold)
    results = results[keep]

    return results.numpy(), windows_lt, (window_height, window_width), (y_num, x_num)


def receive_to_zero(sock: socket.socket):
    data = b""
    while True:
        chunk = sock.recv(1)
        if len(chunk) == 0:
            break

        if chunk == b"\0":
            break
        data += chunk
    return data


def receive_lengeth(sock: socket.socket, length: int):
    data = b""
    while len(data) < length:
        chunk = sock.recv(length - len(data))
        if len(chunk) == 0:
            return b""
        data += chunk
    return data


def main():
    # connect to master
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(30)  # 30s, retry

    host, port = MASTER.split(":")
    port = int(port)
    s.connect((host, port))

    while True:
        head = receive_to_zero(s)
        print(head)

        if len(head) == 0:
            break

        if head == b"ping":
            s.sendall(b"pong\0")

        elif head == b"predict" or head == b"predict_url":
            t00 = time.time()
            img_len = struct.unpack("!I", s.recv(4))[0]
            img = receive_lengeth(s, img_len)

            if head == b"predict_url":
                img = requests.get(img).content

            print("img_received")
            query_len = struct.unpack("!I", s.recv(4))[0]
            query = receive_lengeth(s, query_len)
            print("query_received")
            end = s.recv(1)
            if end != b"\0":
                s.close()
                break

            t_transfer = time.time() - t00

            img = cv2.imdecode(np.frombuffer(img, np.uint8), -1)
            query = json.loads(query)

            print("start_det")
            t0 = time.time()
            boxes, windows_lt, window_size, window_num = detect(
                img,
                query["window_size"],
                query["overlap"],
                query["threshold"],
                query["iou"],
            )
            print("ok")
            resp = json.dumps(
                {
                    "boxes": boxes.tolist(),
                    "windows_lt": windows_lt,
                    "window_size": window_size,
                    "window_num": window_num,
                    "det_time": time.time() - t0,
                    "transfer_time": t_transfer,
                }
            ).encode()
            resp_len = len(resp)

            s.sendall(struct.pack("!I", resp_len))
            s.sendall(resp)
            s.sendall(b"\0")
        else:
            s.close()
            break


if __name__ == "__main__":
    while True:
        try:
            main()
        except Exception as e:
            print(e)
        time.sleep(1)
