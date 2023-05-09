from fastapi import FastAPI, APIRouter, UploadFile, File
from fastapi.staticfiles import StaticFiles
from fastapi.responses import FileResponse, JSONResponse
import uuid
import os
import cv2
from pydantic import BaseModel
import det
import sqlite3
import json
import time
import shutil

conn = sqlite3.connect("data.db", isolation_level=None)

# Ensure Table
conn.execute("CREATE TABLE IF NOT EXISTS detections (id TEXT PRIMARY KEY, params TEXT, modified_at INTEGER, num INTEGER, remark TEXT)")


# Ensure ./detections/ exists
os.makedirs("./detections", exist_ok=True)

app = FastAPI()

api = APIRouter(prefix="/api")


@api.get("/")
def hello():
    return {"message": "Hello World"}


@api.post("/detections")
async def new_detection(file: UploadFile):
    # Create a new detection
    id = str(uuid.uuid4())
    base = os.path.join("./detections", id)
    os.makedirs(base)

    # Convert to jpg
    path = os.path.join(base, "origin.jpg")
    with open(path, "wb") as f:
        f.write(await file.read())
    img = cv2.imread(path)
    cv2.imwrite(path, img)

    params = det.DetParameters()
    print(params.json())

    num = det.detect(params, base)
    # Write Database
    conn.execute("INSERT INTO detections (id, params, modified_at, num, remark) VALUES (?, ?, ?, ?, ?)",
                 (id, params.json(), time.time(), num, ""))

    return {
        "id": id,
        "params": params,
        "modified_at": time.time(),
        "num": num,
        "remark": ""
    }




@api.put("/detections/{id}/params")
async def modify_detection(id: str, params: det.DetParameters):
    # Modify Detection
    row = conn.execute(
        "SELECT * FROM detections WHERE id = ?", (id,)).fetchone()
    if row is None:
        return JSONResponse({"error": "Not Found"}, 404)
    base = os.path.join("./detections", id)
    num = det.detect(params, base)
    conn.execute("UPDATE detections SET params = ?, modified_at = ?, num = ? WHERE id = ?",
                 (params.json(), time.time(), num, id))
    
    return {
        "id": id,
        "params": params,
        "modified_at": time.time(),
        "num": num,
    }

class Remark(BaseModel):
    remark: str

@api.put("/detections/{id}/remark")
async def modify_detection_remark(id: str, remark: Remark):
    # Modify Detection
    row = conn.execute(
        "SELECT * FROM detections WHERE id = ?", (id,)).fetchone()
    if row is None:
        return JSONResponse({"error": "Not Found"}, 404)
    conn.execute("UPDATE detections SET remark = ?, modified_at = ? WHERE id = ?",
                 (remark.remark, time.time(), id))
    
    return {
        "id": id,
        "remark": remark.remark,
        "modified_at": time.time(),
    }


@api.delete("/detections/{id}")
async def delete_detection(id: str):
    base = os.path.join("./detections", id)
    if os.path.exists(base):
        shutil.rmtree(base)
    conn.execute("DELETE FROM detections WHERE id = ?", (id,))
    return {"message": "OK"}


@api.get("/detections/{id}")
async def get_detection(id: str):
    # Get Detection
    row = conn.execute(
        "SELECT * FROM detections WHERE id = ?", (id,)).fetchone()
    if row is None:
        return JSONResponse({"error": "Not Found"}, 404)
    return {
        "id": row[0],
        "params": json.loads(row[1]),
        "modified_at": row[2],
        "num": row[3],
        "remark": row[4]
    }


@api.get("/detections/{id}/origin")
def get_detection_origin_image(id: str):
    # Path: ./detections/{id}/origin.jpg
    path = os.path.join("./detections", id, "origin.jpg")
    if not os.path.exists(path):
        return JSONResponse({"error": "Not Found"}, 404)
    return FileResponse(path)


@api.get("/detections/{id}/boxes")
def get_detection_boxes_image(id: str):
    # Path: ./detections/{id}/boxes.jpg
    path = os.path.join("./detections", id, "boxes.png")
    if not os.path.exists(path):
        return JSONResponse({"error": "Not Found"}, 404)
    return FileResponse(path)

@api.get("/detections/{id}/merged")
def get_detection_merged_image(id: str):
    # Path: ./detections/{id}/merged.jpg
    path = os.path.join("./detections", id, "merged.jpg")
    if not os.path.exists(path):
        return JSONResponse({"error": "Not Found"}, 404)
    return FileResponse(path)

# Serve: /api/detections/{id}/merged_files/{path}
@api.get("/detections/{id}/merged_files/{path:path}")
def get_detection_merged_dzi_files(id: str, path: str):
    # Path: ./detections/{id}/merged.dzi/{path}
    path = os.path.join("./detections", id, "merged_files", path)
    if not os.path.exists(path):
        return JSONResponse({"error": "Not Found"}, 404)
    return FileResponse(path)

@api.get("/detections/{id}/merged.dzi")
def get_detection_merged_dzi(id: str):
    path = os.path.join("./detections", id, "merged.dzi")
    if not os.path.exists(path):
        return JSONResponse({"error": "Not Found"}, 404)
    return FileResponse(path)


@api.get("/detections")
async def get_detections(size: int = 20, page: int = 1):
    # Sort in time order
    rows = conn.execute("SELECT * FROM detections ORDER BY modified_at DESC LIMIT ? OFFSET ?",
                        (size, (page - 1) * size)).fetchall()

    print(rows)

    return list(map(lambda row: {
        "id": row[0],
        "params": json.loads(row[1]),
        "modified_at": row[2],
        "num": row[3],
        "remark": row[4]
    }, rows))


@app.get("/editor/{id}")
def spa():
    return FileResponse("./dist/index.html")

@app.get("/404")
def NotFound():
    return FileResponse("./dist/index.html")


app.include_router(api)

# Include Static Files
app.mount("/", StaticFiles(directory="dist", html=True))

