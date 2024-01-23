import io
from typing import Any, Callable, TypeVar

import inflection
from PIL import Image
import torch
from transformers import AutoTokenizer, Pipeline, PretrainedConfig, pipeline
# from optimum.onnxruntime import ORTModel

T = TypeVar('T')


def _convert_device(data: T, device: torch.device) -> T:
    if isinstance(data, tuple) or isinstance(data, list):
        return [
            _convert_device(item, data)
            for item in data
        ]
    elif isinstance(data, dict):
        return {
            key: _convert_device(value, data)
            for key, value in data.items()
        }
    elif isinstance(data, torch.Tensor):
        return data.to(device)
    else:
        return data


def _replace_payloads(data: T, payloads: list[tuple[str, bytes]]) -> T | bytes | Image.Image:
    if isinstance(data, tuple) or isinstance(data, list):
        return [
            _replace_payloads(item, payloads)
            for item in data
        ]
    elif isinstance(data, dict):
        return {
            key: _replace_payloads(value, payloads)
            for key, value in data.items()
        }
    elif isinstance(data, str):
        scheme = '@data:'
        if isinstance(data, str) and data.startswith(scheme):
            type_, *key = data[len(scheme):].split(',')
            key = ','.join(key)

            data: bytes = next(
                payload_value
                for payload_key, payload_value in payloads
                if payload_key == key
            )

            match type_:
                case 'binary':
                    return data
                case 'image':
                    return Image.open(io.BytesIO(data))
                case _:
                    raise Exception(f'unsupported payload type: {type_}')
        else:
            return data
    else:
        return data


def _select_best_device() -> torch.device:
    return torch.device(
        'cuda'
        if torch.cuda.is_available()
        else 'cpu'
    )


def _parse_task(kind: str) -> str:
    return inflection.dasherize(inflection.underscore(kind))


def load(model_id: str, kind: str) -> Callable:
    scheme = 'huggingface://'
    if model_id.startswith(scheme):
        model_id = model_id[len(scheme):]

    device_from = _select_best_device()
    device_to = torch.device('cpu')

    try:
        tokenizer = AutoTokenizer.from_pretrained(model_id)
    except:
        tokenizer = None

    # model = ORTModel.from_pretrained(
    #     model_id=model_id,
    #     config=PretrainedConfig(
    #         return_dict=True,
    #     ),
    #     export=True,
    #     local_files_only=False,
    # )
    model = model_id

    tick = pipeline(
        task=_parse_task(kind),
        model=model,
        tokenizer=tokenizer,
        device=device_from,
    )
    return lambda inputs: wrap_tick(
        tick=tick,
        inputs=inputs,
        device_from=device_from,
        device_to=device_to,
    )


def preprocess(input: Any) -> dict[str, Any]:
    payloads = input.payloads
    value = input.value

    if not isinstance(value, dict):
        value = {
            'value': value,
        }

    # replace payloads
    return _replace_payloads(value, payloads)


def wrap_tick(
    tick: Pipeline,
    inputs: list[Any],
    device_from: torch.device,
    device_to: torch.device,
) -> list[Any]:
    # skip if empty inputs
    if not inputs:
        return []
    input_type = type(inputs[0])

    outputs = []
    for input in inputs:
        # load inputs
        input_set = _convert_device(preprocess(input), device_from)

        # execute inference
        output_set = _convert_device(tick(**input_set), device_to)

        # flatten outputs
        while isinstance(output_set, list):
            if len(output_set) == 1:
                output_set = output_set[0]
            else:
                break
        if not isinstance(output_set, dict):
            output_set = {
                'value': output_set,
            }

        # pack outputs
        output = input_type(
            input.payloads,
            output_set,
            input.reply,
        )
        outputs.append(output)
    return outputs


# Debug
if __name__ == '__main__':
    tick = load(
        model_id='huggingface://deepset/roberta-base-squad2',
        kind='QuestionAnswering',
    )

    class DummyMessage:
        def __init__(
            self,
            payloads: list[tuple[str, bytes]],
            value: Any,
            reply: str | None = None,
        ) -> None:
            self.payloads = payloads
            self.value = value
            self.reply = reply

        def __repr__(self) -> str:
            return repr({
                'payloads': {
                    payload_key: len(payload_value)
                    for payload_key, payload_value in self.payloads
                },
                'value': self.value,
            })

    inputs = [
        DummyMessage(
            payloads=[],
            value={
                'context': 'I am happy.',
                'question': 'What is my feel?',
            },
        ),
    ]

    outputs = tick(inputs)
    print(outputs)
