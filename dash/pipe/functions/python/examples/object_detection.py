#!/usr/bin/env python3


import io
import json
from typing import Any

from PIL import Image
import torch


def _init_processor() -> torch.device:
    if torch.cuda.is_available():
        return torch.device('cuda')


def _init_model() -> Any:
    return torch.hub.load('ultralytics/yolov5', 'yolov5s', pretrained=True) \
        .to(device=_init_processor())


# load model(s)
model = _init_model()


def tick(inputs: list[Any]) -> list[Any]:
    # skip if empty inputs
    if not inputs:
        return []
    input_type = type(inputs[0])

    # load payloads
    input_set: list[tuple[int, int, str, bytes]] = [
        (
            batch_idx,
            payload_idx,
            key,
            payload,
        )
        for batch_idx, input in enumerate(inputs)
        for payload_idx, (key, payload) in enumerate(input.payloads)
    ]

    # skip if empty payloads
    if not input_set:
        return []

    # load inputs
    input_tensor = [
        Image.open(io.BytesIO(payload))
        for (_, _, _, payload) in input_set
    ]

    # execute inference
    output_set = model(input_tensor, size=640).pandas().xyxy

    # pack payloads
    outputs = []
    for (batch_idx, payload_idx, key, payload), output in zip(input_set, output_set):
        output_payloads = [(key, None)]
        output_value = {
            'key': key,
            'value': json.loads(output.to_json(orient='records')),
        }
        outputs.append((output_payloads, output_value))

    return [
        input_type(output_payloads, output_value)
        for output_payloads, output_value in outputs
    ]
