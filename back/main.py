from multiprocessing import Queue
from sanic import Sanic, json, file, Blueprint, Request

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
from config import DB, MASTER

conn = psycopg2.connect(DB, cursor_factory=psycopg2.extras.RealDictCursor)

conn.autocommit = True

# Ensure Table
with conn.cursor() as c:
    c.execute(
        """
        CREATE TABLE IF NOT EXISTS detections (id TEXT PRIMARY KEY, params TEXT, modified_at INTEGER, num INTEGER, remark TEXT, status TEXT)
        """
    )

# Ensure ./detections/ exists
os.makedirs("./detections", exist_ok=True)

app = Sanic(__name__)
Sanic.start_method = "fork"
api = Blueprint("api", url_prefix="/api")


def nms(boxes, threshold, iou):  # boxes: [x1, y1, x2, y2, score]
    boxes = np.array(boxes)
    # filter boxes with low score
    boxes = boxes[boxes[:, 4] > threshold]

    x1 = boxes[:, 0]
    y1 = boxes[:, 1]
    x2 = boxes[:, 2]
    y2 = boxes[:, 3]
    scores = boxes[:, 4]

    areas = (y2 - y1) * (x2 - x1)
    result = []

    index = scores.argsort()[::-1]
    while index.size > 0:
        i = index[0]
        result.append(i)
        xx1 = np.maximum(x1[i], x1[index[1:]])
        yy1 = np.maximum(y1[i], y1[index[1:]])
        xx2 = np.minimum(x2[i], x2[index[1:]])
        yy2 = np.minimum(y2[i], y2[index[1:]])
        w = np.maximum(0.0, xx2 - xx1)
        h = np.maximum(0.0, yy2 - yy1)
        inter = w * h
        ovr = inter / (areas[i] + areas[index[1:]] - inter)
        inds = np.where(ovr <= iou)[0]
        index = index[inds + 1]

    # to list
    return boxes[result].tolist()


class PredictionTask:
    id: str
    params: dict

    def __init__(self, id: str, params: dict):
        self.id = id
        self.params = params

    @property
    def db(self):
        return conn

    def image_url(self):
        return f"http://dev.imlihe.com:20000/api/detections/{self.id}/origin"

    def set_status(self, status):
        with self.db.cursor() as c:
            c.execute(
                "UPDATE detections SET status = %s WHERE id = %s", (status, self.id)
            )

    # def done(self, boxes, window_num, window_size, window_lt):
    #

    def nms_only(self):
        base = os.path.join("./detections", self.id)
        with open(os.path.join(base, "result.json"), "r") as f:
            result = mjson.loads(f.read())

        self.done(result)

    def done(self, result):
        print("Done", self.id, self.params)
        base = os.path.join("./detections", self.id)
        os.makedirs(base, exist_ok=True)
        boxes = result["boxes"]

        with open(os.path.join(base, "result.json"), "w") as f:
            f.write(mjson.dumps(result))

        boxes = nms(boxes, self.params["threshold"], self.params["iou"])

        with open(os.path.join(base, "boxes.json"), "w") as f:
            f.write(mjson.dumps(boxes))

        # draw boxes
        img = cv2.imread(os.path.join(base, "origin.jpg"))
        for box in boxes:
            x1, y1, x2, y2, score = box
            x1, y1, x2, y2 = int(x1), int(y1), int(x2), int(y2)
            cv2.rectangle(img, (x1, y1), (x2, y2), (0, 255, 0), 2)
            cv2.putText(
                img,
                f"{score:.2f}",
                (x1, y1),
                cv2.FONT_HERSHEY_SIMPLEX,
                0.5,
                (0, 255, 0),
                2,
            )

        cv2.imwrite(os.path.join(base, "origin.boxes.jpg"), img)

        # draw windows
        window_size = result["window_size"]
        windows_lt = result["windows_lt"]
        for x, y in windows_lt:
            x, y = int(x), int(y)
            h, w = int(window_size[0]), int(window_size[1])
            cv2.rectangle(img, (x, y), (x + w, y + h), (255, 0, 0), 2)
        cv2.imwrite(os.path.join(base, "origin.windows.jpg"), img)

        with self.db.cursor() as c:
            c.execute(
                "UPDATE detections SET num = %s, status = %s WHERE id = %s",
                (len(boxes), "done", self.id),
            )


@app.main_process_start
async def demo_task(app: Sanic):
    app.shared_ctx.queue = Queue()
    print("Main")

    p = master.Pool(MASTER, app.shared_ctx.queue)
    p.start()

    # find all queue tasks
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE status = 'queue'")
        rows = c.fetchall()

    print("Queue", rows)
    for row in rows:
        app.shared_ctx.queue.put(PredictionTask(row["id"], mjson.loads(row["params"])))


@api.get("/")
async def hello(request: Request):
    return json({"message": "Hello World"})


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
        "window_size": 0.5,
        "overlap": 0.1,
        "threshold": 0.5,
        "iou": 0.5,
    }
    # detect

    # Write Database
    with conn.cursor() as c:
        c.execute(
            "insert into detections (id, params, modified_at, remark, status) values (%s, %s, %s, %s, %s)",
            (id, mjson.dumps(params), time.time(), "", "queue"),
        )

    # Add Task
    request.app.shared_ctx.queue.put(PredictionTask(id, params))

    return json(
        {
            "id": id,
            "params": params,
            "modified_at": time.time(),
            "status": "queue",
            "queue": request.app.shared_ctx.queue.qsize(),
            "remark": "",
        }
    )


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

    # Modify detection
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()

    if row is None:
        return json({"error": "Not Found"}, 404)

    row["params"] = mjson.loads(row["params"])
    old_params = row["params"]

    if old_params == params:
        return json(row)

    with conn.cursor() as c:
        c.execute(
            "UPDATE detections SET params = %s, modified_at = %s, num = NULL, status = %s WHERE id = %s",
            (mjson.dumps(params), time.time(), "queue", id),
        )

    task = PredictionTask(id, params)
    if (
        old_params["window_size"] == params["window_size"]
        and old_params["overlap"] == params["overlap"]
        and old_params["tiling"] == params["tiling"]
    ):
        task.nms_only()
        return _get_detection(request, id)

    request.app.shared_ctx.queue.put(task)

    return _get_detection(request, id)


@api.put("/detections/<id>/remark")
async def modify_detection_remark(request: Request, id: str):
    # Modify Detection
    remark = request.json.get("remark", "")

    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()
        if row is None:
            return json({"error": "Not Found"}, 404)
        c.execute(
            "UPDATE detections SET remark = %s, modified_at = %s WHERE id = %s",
            (remark, time.time(), id),
        )

    return _get_detection(request, id)


@api.delete("/detections/<id>")
async def delete_detection(_, id: str):
    base = os.path.join("./detections", id)
    if os.path.exists(base):
        shutil.rmtree(base)

    with conn.cursor() as c:
        c.execute("DELETE FROM detections WHERE id = %s", (id,))

    return json({"id": id})


def _get_detection(request: Request, id: str):
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute("SELECT * FROM detections WHERE id = %s", (id,))
        row = c.fetchone()
    if row is None:
        return json({"error": "Not Found"}, 404)
    row["params"] = mjson.loads(row["params"])
    if row["status"] == "queue":
        row["queue"] = request.app.shared_ctx.queue.qsize()
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
    # Sort in time order
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
        c.execute(
            "SELECT * FROM detections ORDER BY modified_at DESC LIMIT %s OFFSET %s",
            (size, (page - 1) * size),
        )
        rows = c.fetchall()

    print(rows)

    return json(rows)


app.blueprint(api)
