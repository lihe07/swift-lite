from sanic import Sanic, json, file, Blueprint, Request

import threading
import uuid
import os
import cv2
import numpy as np
import master
import psycopg2
import psycopg2.extras
import time
import shutil
import json as mjson
from config import UPDATE_PIPE
import socketio
import aiofiles
from common import make_conn, _update_detection, PredictionTask
from asyncio import Queue
import asyncio


if not os.path.exists(UPDATE_PIPE):
    os.mkfifo(UPDATE_PIPE)


# Ensure Table
with make_conn().cursor() as c:
    c.execute(
        """
        CREATE TABLE IF NOT EXISTS detections (id TEXT PRIMARY KEY, params TEXT, modified_at INTEGER, num INTEGER, remark TEXT, status TEXT)
        """
    )

    c.execute(
        """
        CREATE TABLE IF NOT EXISTS april_fools (id SERIAL PRIMARY KEY, created_at TIMESTAMP)
        """
    )

# Ensure ./detections/ exists
os.makedirs("./detections", exist_ok=True)

sio = socketio.AsyncServer(
    async_mode="sanic",
    logger=True,
    cors_allowed_origins="*",
    ping_timeout=5,
    ping_interval=5,
)
app = Sanic(__name__)
Sanic.start_method = "fork"
sio.attach(app, "/api/ws")

api = Blueprint("api", url_prefix="/api")


@app.before_server_start
async def before_server_start(app: Sanic, loop):
    async def _emitter():
        while True:
            # read from pipe, asyncio
            async with aiofiles.open(UPDATE_PIPE, "r") as f:
                id = await f.readline()
                id = id.strip()
                if not id:
                    continue

            print("Emit", id)
            db = make_conn()
            with db.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
                c.execute("SELECT * FROM detections WHERE id = %s", (id,))
                row = c.fetchone()

            if row is None:
                continue

            row["params"] = mjson.loads(row["params"])

            print("Emit", row)

            # await sio.emit("update_detection", row, room=id)
            try:
                await asyncio.wait_for(sio.emit("update_detection", id, room="all"), 3)
            except asyncio.TimeoutError:
                print("Timeout, skipping emit")

    sio.start_background_task(_emitter)

    app.ctx.tasks = Queue()

    app.add_task(master.accept_loop(app.ctx.tasks))
    # find all queue tasks
    conn = make_conn()
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE status = 'queue'")
        rows = c.fetchall()

    print("Queue", rows, app.ctx.tasks.qsize())
    for row in rows:
        await app.ctx.tasks.put(row["id"])

    print("ok", app.ctx.tasks.qsize())


@sio.on("join")
async def join(sid, detection_id):
    print(threading.current_thread())
    await sio.enter_room(sid, detection_id)


@api.get("/")
async def hello(_: Request):
    return json({"message": "Hello World"})


@api.post("/april-fools")
async def add_april_fools(_):
    conn = make_conn()
    with conn.cursor() as c:
        c.execute("INSERT INTO april_fools (created_at) VALUES (NOW())")
        c.execute("SELECT COUNT(*) FROM april_fools")
        count = c.fetchone()["count"]

    return json({"count": count})


@api.post("/detections")
async def new_detection(request: Request):
    # check file
    if not request.files or "file" not in request.files:
        return json({"error": "No Image"}, 400)

    # Create a new detection
    id = str(uuid.uuid4())
    base = os.path.join("./detections", id)
    os.makedirs(base)

    img = cv2.imdecode(np.frombuffer(request.files["file"][0].body, np.uint8), -1)

    # Convert to jpg
    path = os.path.join(base, "origin.jpg")
    cv2.imwrite(path, img)

    params = {
        "tiling": True,
        "window_size": 0.3,
        "overlap": 0.1,
        "threshold": 0.3,
        "iou": 0.5,
    }
    conn = make_conn()
    # Write Database
    with conn.cursor() as c:
        c.execute(
            "INSERT INTO detections (id, params, modified_at, remark, status) values (%s, %s, %s, %s, %s)",
            (id, mjson.dumps(params), time.time(), "", "queue"),
        )

    # Add Task
    print("Writing to pipe", id)
    await request.app.ctx.tasks.put(id)

    await sio.emit(
        "new_detection",
        id,
        room="all",
    )

    return _get_detection(request, id)


@api.put("/detections/<id>/params")
async def modify_detection(request: Request, id: str):
    params = request.json

    # Check params
    keys = ["tiling", "window_size", "overlap", "threshold", "iou"]
    for k in keys:
        if k not in params:
            return json({"error": f"Missing {k}"}, 400)

    if not isinstance(params["tiling"], bool):
        return json({"error": "tiling should be bool"}, 400)

    # window_size should be within (0, 1]
    if not 0 < params["window_size"] <= 1:
        return json({"error": "window_size should be within (0, 1]"}, 400)

    # overlap should be within [0, 1)
    if not 0 <= params["overlap"] < 1:
        return json({"error": "overlap should be within [0, 1)"}, 400)

    # threshold should be within [0, 1]
    if not 0 <= params["threshold"] <= 1:
        return json({"error": "threshold should be within [0, 1]"}, 400)

    # iou should be within [0, 1]
    if not 0 <= params["iou"] <= 1:
        return json({"error": "iou should be within [0, 1]"}, 400)

    conn = make_conn()
    # Modify detection
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()

    if row is None:
        return json({"error": "Not Found"}, 404)

    if row["status"] != "done":
        # No change
        return _get_detection(request, id)

    row["params"] = mjson.loads(row["params"])
    old_params = row["params"]

    if old_params == params:
        return json(row)

    _update_detection(id, params=mjson.dumps(params), status="queue")

    if (
        old_params["window_size"] == params["window_size"]
        and old_params["overlap"] == params["overlap"]
        and old_params["tiling"] == params["tiling"]
    ):
        task = PredictionTask(id, params)
        task.nms_only()
        return _get_detection(request, id)

    await request.app.ctx.tasks.put(id)

    return _get_detection(request, id)


@api.put("/detections/<id>/remark")
async def modify_detection_remark(request: Request, id: str):
    # Modify Detection
    remark = request.json.get("remark", "")

    conn = make_conn()
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()
        if row is None:
            return json({"error": "Not Found"}, 404)
        _update_detection(id, remark=remark)

    return _get_detection(request, id)


@api.delete("/detections/<id>")
async def delete_detection(_, id: str):
    base = os.path.join("./detections", id)
    if os.path.exists(base):
        shutil.rmtree(base)

    conn = make_conn()
    with conn.cursor() as c:
        c.execute("DELETE FROM detections WHERE id = %s", (id,))

    return json({"id": id})


def _get_detection(request: Request, id: str):
    conn = make_conn()
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()
    if row is None:
        return json({"error": "Not Found"}, 404)
    row["params"] = mjson.loads(row["params"])
    if row["status"] == "queue":
        row["queue"] = 1
    return json(row)


@api.get("/detections/<id>")
async def get_detection(r: Request, id: str):
    return _get_detection(r, id)


IMAGE_TYPES = {
    "origin": "origin.jpg",
    "boxes": "origin.boxes.jpg",
    "windows": "origin.windows.jpg",
}


@api.get("/detections/<id>/<im>")
def get_detection_origin_image(_, id: str, im: str):
    # Path: ./detections/{id}/origin.jpg

    if im not in IMAGE_TYPES:
        return json({"error": "Not Found"}, 404)

    path = os.path.join("./detections", id, IMAGE_TYPES[im])
    if not os.path.exists(path):
        return json({"error": "Not Found"}, 404)

    return file(path, max_age=2629746)


@api.get("/detections")
async def get_detections(request: Request):
    size = int(request.args.get("size", 20))
    page = int(request.args.get("page", 1))
    sortby = request.args.get("sortby", "modified_at")
    sort = request.args.get("sort", "desc")

    # validate
    if size < 1 or size > 100:
        return json({"error": "Size should be within [1, 100]"}, 400)

    if page < 1:
        return json({"error": "Page should be greater than 0"}, 400)

    if sortby not in ["modified_at", "num", "status"]:
        return json({"error": "Invalid sortby"}, 400)

    if sort not in ["asc", "desc"]:
        return json({"error": "Invalid sort"}, 400)

    conn = make_conn()
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute(
            f"SELECT id,num,modified_at,remark,status FROM detections ORDER BY {sortby} {sort} NULLS LAST LIMIT %s OFFSET %s",
            (size, (page - 1) * size),
        )
        rows = c.fetchall()
        # get total
        c.execute("SELECT COUNT(*) FROM detections")
        total = c.fetchone()["count"]

    return json({"total": total, "data": rows})


app.blueprint(api)
