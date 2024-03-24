from config import DB, UPDATE_PIPE, BASE

import psycopg2
import psycopg2.extras
import time
import numpy as np
import cv2
import os
import json


def make_conn():
    conn = psycopg2.connect(DB, cursor_factory=psycopg2.extras.RealDictCursor)
    conn.autocommit = True
    return conn


def _update_detection(id, num=None, status=None, remark=None, params=None):
    db = make_conn()
    sql = "UPDATE detections SET "

    vars = []

    if num is not None:
        sql += "num = %s, "
        vars.append(num)
    if status is not None:
        sql += "status = %s, "
        vars.append(status)
    if remark is not None:
        sql += "remark = %s, "
        vars.append(remark)
    if params is not None:
        sql += "params = %s, "
        vars.append(params)

    sql += "modified_at = %s WHERE id = %s"

    vars.append(time.time())
    vars.append(id)

    with db.cursor() as c:
        c.execute(sql, vars)

    db.commit()

    print(sql, vars)

    # write to pipe
    with open(UPDATE_PIPE, "w") as f:
        f.write(id + "\n")


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

    @staticmethod
    def from_id(id: str):
        conn = make_conn()
        with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as c:
            c.execute("SELECT * FROM detections WHERE id = %s", (id,))
            row = c.fetchone()
        if row is None:
            return None

        return PredictionTask(row["id"], json.loads(row["params"]))

    def image_url(self):
        # return f"{BASE}/api/detections/{self.id}/origin"
        return f"./detections/{self.id}/origin.jpg"

    def nms_only(self):
        base = os.path.join("./detections", self.id)
        with open(os.path.join(base, "result.json"), "r") as f:
            result = json.loads(f.read())

        self.done(result)

    def set_status(self, status):
        _update_detection(self.id, status=status)

    def done(self, result):
        print("Done", self.id, self.params)
        base = os.path.join("./detections", self.id)
        os.makedirs(base, exist_ok=True)
        boxes = result["boxes"]

        with open(os.path.join(base, "result.json"), "w") as f:
            f.write(json.dumps(result))

        boxes = nms(boxes, self.params["threshold"], self.params["iou"])

        with open(os.path.join(base, "boxes.json"), "w") as f:
            f.write(json.dumps(boxes))

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

        _update_detection(self.id, num=len(boxes), status="done")
