import io
import json
from typing import Any, Callable

from PIL import Image
from transformers import AutoTokenizer, Pipeline, PretrainedConfig, pipeline
from optimum.onnxruntime import ORTModel


def _parse_task(kind: str) -> str:
    match kind:
        case 'QuestionAnswering':
            return 'question-answering'
        case 'Summarization':
            return 'summarization'
        case 'TextGeneration':
            return 'text-generation'
        case 'Translation':
            return 'translation'
        case 'ZeroShotClassification':
            return 'zero-shot-classification'


def load(model_id: str, kind: str) -> Callable:
    scheme = 'huggingface://'
    if model_id.startswith(scheme):
        model_id = model_id[len(scheme):]

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
    )
    return lambda inputs: wrap_tick(
        tick=tick,
        inputs=inputs,
        kind=kind,
    )


def preprocess(input: Any, kind: str) -> dict[str, Any]:
    match kind:
        # NLP
        case 'QuestionAnswering' | \
                'Translation':
            return input.value


def postprocess(
    input_type: type,
    input: Any,
    output_set: dict[str, Any],
    kind: str,
) -> Any:
    # pack payloads
    return input_type(
        input.payloads,
        output_set,
        input.reply,
    )


def wrap_tick(
    tick: Pipeline,
    inputs: list[Any],
    kind: str,
) -> list[Any]:
    # skip if empty inputs
    if not inputs:
        return []
    input_type = type(inputs[0])

    outputs = []
    for input in inputs:
        # load inputs
        input_set = preprocess(input, kind)

        # execute inference
        output_set = tick(input_set)

        # pack outputs
        output = postprocess(input_type, input, output_set, kind)
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
            payloads: list[Any],
            value: Any,
            reply: str | None,
        ) -> None:
            self.payloads = payloads
            self.value = value
            self.reply = reply

        def __repr__(self) -> str:
            return repr({
                'payloads': self.payloads,
                'value': self.value,
            })

    inputs = [
        DummyMessage(
            payloads=[],
            value={
                'context': 'I am happy.',
                'question': 'What is my feel?',
            },
            reply=None,
        ),
    ]

    outputs = tick(inputs)
    print(outputs)
