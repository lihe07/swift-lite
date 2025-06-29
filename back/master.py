import multiprocessing
import socket
import struct
import json
from copy import deepcopy
from common import PredictionTask
from config import MASTER
import queue

import threading


class Worker(threading.Thread):
    def __init__(
        self,
        s: socket.socket,
        q: multiprocessing.Queue,
    ):
        super().__init__()
        self.s = s
        self.q = q

    def receive_or_timeout(self, size: int, timeout: float):
        self.s.settimeout(timeout)

        def recv_by_chunks():
            data = b""
            while len(data) < size:
                chunk = self.s.recv(size - len(data))
                if len(chunk) == 0:
                    return None
                data += chunk
            return data

        try:
            return recv_by_chunks()
        except socket.timeout:
            return None

    def ping(self):
        self.s.sendall(b"ping\0")

        response = self.receive_or_timeout(5, 3)
        print(response)
        if response != b"pong\0":
            return False

        return True

    def close(self):
        try:
            self.s.shutdown(socket.SHUT_RDWR)
            self.s.close()
        except Exception as e:
            print("Failed to close", e, "Already closed?")

    def predict(
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
        query_bytes = json.dumps(query).encode()
        query_len = len(query_bytes)
        self.s.sendall(struct.pack("!I", query_len))
        self.s.sendall(query_bytes)
        self.s.sendall(b"\0")

        print("Waiting for response")

        # Wait
        size = self.receive_or_timeout(4, 20)
        if size is None:
            return None

        size = struct.unpack("!I", size)[0]
        data = self.receive_or_timeout(size, 10)
        if data is None:
            return None

        end = self.receive_or_timeout(1, 10)

        if end != b"\0":
            return None

        print("Ok")

        return json.loads(data)

    def read_task(self, timeout: float):
        print("Waiting for task...", self.q.qsize())
        try:
            task = self.q.get(timeout=timeout)
            print("Got task", task)
            return PredictionTask.from_id(task.strip())
        except queue.Empty:
            return None

    def run(self) -> None:
        if not self.ping():
            return

        print("Qsize", self.q.qsize())
        while True:
            try:
                task = self.read_task(timeout=5)

                if task is None:
                    print("No task, checking ping")
                    if not self.ping():
                        print("Worker failed to ping")
                        break
                    continue

                # check if I am still alive
                if not self.ping():
                    print("Worker failed to ping")
                    task.set_status("queue")
                    self.q.put(task.id)
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
                    self.q.put(task.id)
                    # kill the worker
                    break
                task.done(result)
                print("Det time", task.id, result["det_time"])

            except Exception as e:
                print(e)
                break

        self.close()


class MasterProcess(multiprocessing.Process):
    def __init__(self, q: multiprocessing.Queue):
        super().__init__()
        self.daemon = True
        self.q = q

    def run(self):
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        s.bind(MASTER)
        s.listen(5)
        workers = []

        print("Accepting on", MASTER)
        # w = Worker(None, loop, q)
        # loop.create_task(w.handler())

        # accepting loop
        while True:
            conn, addr = s.accept()
            print("Connection from", addr)
            w = Worker(conn, self.q)
            w.start()

            workers.append(w)
