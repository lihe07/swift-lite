try:
    from sanic import Sanic, Request, json as rjson

    app = Sanic(__name__)

    @app.post("/predict")
    async def predict(request: Request):
        image = request.files and request.files.get("image")

        if not image:
            return rjson({"error": "No image"})

        window_size = float(request.args.get("window_size", 1.0))
        overlap = float(request.args.get("overlap", 0.1))
        threshold = float(request.args.get("threshold", 0.3))
        iou = float(request.args.get("iou", 0.5))

        # decode
        img = cv2.imdecode(np.frombuffer(image.body, np.uint8), -1)
        boxes, windows_lt, window_size = detect(
            img, window_size, overlap, threshold, iou
        )

        return rjson(
            {
                "boxes": boxes.tolist(),
                "windows_lt": windows_lt,
                "window_size": window_size,
            }
        )

except ImportError:
    pass
