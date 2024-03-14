import socket
import struct
import json
import threading
import cv2

workers = []


class Worker:
    def __init__(self, s: socket.socket):
        self.s = s
        self.mutex = threading.Lock()

    def receive_or_timeout(self, size: int, timeout: float):
        self.s.settimeout(timeout)
        try:
            data = b""
            while len(data) < size:
                chunk = self.s.recv(size - len(data))
                if len(chunk) == 0:
                    return None
                data += chunk
        except socket.timeout:
            self.s.settimeout(None)
            return None
        self.s.settimeout(None)
        return data

    def _ping(self):
        self.s.sendall(b"ping\0")
        response = self.receive_or_timeout(5, 10)
        print(response)
        if response != b"pong\0":
            self.s.close()
            return False

        return True

    def ping(self):
        self.mutex.acquire()
        result = self._ping()
        self.mutex.release()
        return result

    def _predict(
        self,
        img_path: str,
        window_size: float,
        overlap: float,
        threshold: float,
        iou: float,
    ):
        print("Sending predict")
        if img_path.startswith("http"):
            self.s.sendall(b"predict_url\0")
            url = img_path.encode()
            url_len = len(url)
            self.s.sendall(struct.pack("!I", url_len))
            self.s.sendall(url)
        else:
            self.s.sendall(b"predict\0")
            with open(img_path, "rb") as f:
                img = f.read()
                img_len = len(img)
                self.s.sendall(struct.pack("!I", img_len))
                self.s.sendall(img)
        # send json
        query = json.dumps(
            {
                "window_size": window_size,
                "overlap": overlap,
                "threshold": threshold,
                "iou": iou,
            }
        ).encode()
        query_len = len(query)
        self.s.sendall(struct.pack("!I", query_len))
        self.s.sendall(query)
        self.s.sendall(b"\0")

        # Wait
        size = self.receive_or_timeout(4, 20)
        if size is None:
            self.s.close()
            return None

        size = struct.unpack("!I", size)[0]
        data = self.receive_or_timeout(size, 10)
        if data is None:
            self.s.close()
            return None
        end = self.s.recv(1)
        if end != b"\0":
            self.s.close()
            return None

        return json.loads(data)

    def predict(
        self,
        img_path: str,
        window_size: float,
        overlap: float,
        threshold: float,
        iou: float,
    ):
        self.mutex.acquire()
        result = self._predict(img_path, window_size, overlap, threshold, iou)
        self.mutex.release()
        return result


# listen for workers
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

s.bind(("0.0.0.0", 12345))

s.listen(1)
conn, addr = s.accept()
print("Connected by", addr)
w = Worker(conn)
if not w.ping():
    print("Worker", addr, "failed to ping")
    exit(0)


resp = w.predict(
    # "https://lms.d.zhan.com/zhanlms/addon_homework/2024/03/151623565f0f8847456f/big.jpg",
    "https://lite.bwrrc.org.cn/big.jpg",
    # "/home/lihe07/Desktop/HIC-Yolo/big.jpg",
    0.1,
    0.1,
    0.3,
    1.0,
)
print(resp)

if resp:
    im = cv2.imread("/home/lihe07/Desktop/HIC-Yolo/big.jpg")
    for box in resp["boxes"]:
        x1, y1, x2, y2 = map(int, box[:4])
        cv2.rectangle(im, (x1, y1), (x2, y2), (0, 255, 0), 2)

    cv2.imwrite("./result.jpg", im)

    result = resp
    window_size = result["window_size"]
    windows_lt = result["windows_lt"]
    for x, y in windows_lt:
        x, y = int(x), int(y)
        h, w = int(window_size[0]), int(window_size[1])
        cv2.rectangle(im, (x, y), (x + w, y + h), (255, 0, 0), 2)

    cv2.imwrite("./windows.jpg", im)
    print(len(resp["boxes"]))
