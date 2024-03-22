import socket
import struct
import json
import threading
from copy import deepcopy
from multiprocessing import Queue


workers = []


class Worker(threading.Thread):
    def __init__(self, s: socket.socket, q: Queue):
        self.s = s
        self.q = q
        self.mutex = threading.Lock()
        super().__init__()

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
        query: dict,
    ):
        print("Sending predict", img_path)
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
        query = json.dumps(query).encode()
        query_len = len(query)
        self.s.sendall(struct.pack("!I", query_len))
        self.s.sendall(query)
        self.s.sendall(b"\0")

        print("Waiting for response")

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

        print("Ok")

        return json.loads(data)

    def predict(
        self,
        img_path: str,
        params: dict,
    ):
        self.mutex.acquire()
        result = self._predict(img_path, params)
        self.mutex.release()
        return result

    def run(self) -> None:
        while True:
            try:
                print("Waiting for task")
                task = self.q.get()
                # check if I am still alive
                print("Got task", task)
                if not self.ping():
                    print("Worker failed to ping")
                    self.q.put(task)
                    self.s.close()
                    break
                print("Predicting", task.id)
                task.set_status("processing")

                url = task.image_url()
                params = deepcopy(task.params)

                if not params.get("tiling", True):
                    params["window_size"] = 1.0
                    params["overlap"] = 0.0

                params["threshold"] = 0.05
                params["iou"] = 0.95

                result = self.predict(url, params)
                if result is None:
                    print("Worker failed to predict")
                    # push back to queue
                    task.set_status("queue")
                    self.q.put(task)
                    break
                task.done(result)
                print("Det time", task.id, result["det_time"])

            except Exception as e:
                print(e)
                break


class Pool(threading.Thread):
    workers = []

    def __init__(self, bind: tuple, q: Queue):
        self.bind = bind
        self.q = q
        super().__init__()

    def run(self) -> None:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind(self.bind)
        s.listen(1)

        # accepting loop
        while True:
            conn, addr = s.accept()
            print("Connection from", addr)
            w = Worker(conn, self.q)
            if not w.ping():
                print("Worker", addr, "failed to ping")
                exit(0)
            w.start()
            self.workers.append(w)
