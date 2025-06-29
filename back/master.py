import socket
import struct
import json
from copy import deepcopy
from common import PredictionTask
from config import MASTER
import asyncio

print("Hello")


class Worker:
    def __init__(self, s: socket.socket, loop: asyncio.AbstractEventLoop, q):
        self.s = s
        self.loop = loop
        self.q = q

    async def receive_or_timeout(self, size: int, timeout: float):
        async def recv_by_chunks():
            data = b""
            while len(data) < size:
                chunk = await self.loop.sock_recv(self.s, size - len(data))
                if len(chunk) == 0:
                    return None
                data += chunk
            return data

        try:
            return await asyncio.wait_for(recv_by_chunks(), timeout)
        except asyncio.TimeoutError:
            return None

    async def ping(self):
        await self.loop.sock_sendall(self.s, b"ping\0")

        response = await self.receive_or_timeout(5, 3)
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

    async def predict(
        self,
        img_path: str,
        query: dict,
    ):
        print("Sending predict", img_path)
        if img_path.startswith("http"):
            # self.s.sendall(b"predict_url\0")
            await self.loop.sock_sendall(self.s, b"predict_url\0")
            url = img_path.encode()
            url_len = len(url)
            # self.s.sendall(struct.pack("!I", url_len))
            # self.s.sendall(url)
            await self.loop.sock_sendall(self.s, struct.pack("!I", url_len))
            await self.loop.sock_sendall(self.s, url)
        else:
            # self.s.sendall(b"predict\0")
            await self.loop.sock_sendall(self.s, b"predict\0")
            with open(img_path, "rb") as f:
                img = f.read()
                img_len = len(img)
                # self.s.sendall(struct.pack("!I", img_len))
                # self.s.sendall(img)
                await self.loop.sock_sendall(self.s, struct.pack("!I", img_len))
                await self.loop.sock_sendall(self.s, img)

        # send json
        query_bytes = json.dumps(query).encode()
        query_len = len(query_bytes)
        # self.s.sendall(struct.pack("!I", query_len))
        # self.s.sendall(query)
        # self.s.sendall(b"\0")
        await self.loop.sock_sendall(self.s, struct.pack("!I", query_len))
        await self.loop.sock_sendall(self.s, query_bytes)
        await self.loop.sock_sendall(self.s, b"\0")

        print("Waiting for response")

        # Wait
        size = await self.receive_or_timeout(4, 20)
        if size is None:
            return None

        size = struct.unpack("!I", size)[0]
        data = await self.receive_or_timeout(size, 10)
        if data is None:
            return None

        # end = self.s.recv(1)
        end = await self.receive_or_timeout(1, 10)

        if end != b"\0":
            return None

        print("Ok")

        return json.loads(data)

    async def read_task(self):
        # async with aiofiles.open(TASK_PIPE, "r") as pipe:
        #     task = await pipe.readline()
        print("Waiting for task...", self.q.qsize())
        task = await self.q.get()
        print("Got task", task)
        return PredictionTask.from_id(task.strip())

    async def handler(self) -> None:
        print("Qsize", self.q.qsize())
        while True:
            try:
                try:
                    task = await asyncio.wait_for(self.read_task(), 10)

                    if task is None:
                        continue
                except asyncio.TimeoutError:
                    print("No task, checking ping")
                    if not await self.ping():
                        print("Worker failed to ping")
                        break
                    continue

                # check if I am still alive
                if not await self.ping():
                    print("Worker failed to ping")
                    task.set_status("queue")
                    await self.q.put(task.id)
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

                result = await self.predict(url, params)
                if result is None:
                    print("Worker failed to predict")
                    # push back to queue
                    task.set_status("queue")
                    await self.q.put(task.id)
                    # kill the worker
                    break
                task.done(result)
                print("Det time", task.id, result["det_time"])

            except Exception as e:
                print(e)
                break

        self.close()


async def accept_loop(q):
    loop = asyncio.get_event_loop()
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    # set non-blocking
    s.setblocking(False)
    s.bind(MASTER)
    s.listen(5)
    workers = []

    print("Accepting on", MASTER)
    # w = Worker(None, loop, q)
    # loop.create_task(w.handler())

    # accepting loop
    while True:
        conn, addr = await loop.sock_accept(s)
        print("Connection from", addr)
        w = Worker(conn, loop, q)
        if not await w.ping():
            print("Worker", addr, "failed to ping")
            w.close()
            continue
        loop.create_task(w.handler())
        workers.append(w)
