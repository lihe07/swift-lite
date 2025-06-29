import torch

MODEL = "./best.torchscript.pt"
model = torch.jit.load(MODEL)

input_names = ["input"]
output_names = ["output"]

BATCH_SIZE = 32
dummy_input = torch.randn(BATCH_SIZE, 3, 640, 640)  # Adjust the shape as needed

torch.onnx.export(
    model,
    dummy_input,
    "./best.onnx",
    verbose=True,
    input_names=input_names,
    output_names=output_names,
    dynamic_axes={"input": [0], "output": [0]},
)
