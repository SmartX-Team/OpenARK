use actix_multipart::form::{text::Text, MultipartForm};
use ipis::{
    async_trait::async_trait,
    core::{
        anyhow::{bail, Result},
        ndarray,
    },
    futures::TryFutureExt,
    itertools::Itertools,
};
use ort::tensor::InputTensor;
use serde::Serialize;

use crate::tensor::{OutputTensor, TensorType};

pub(crate) struct Solver {
    base: super::SolverBase,
}

impl Solver {
    pub(crate) async fn load_from_huggingface(repo: &str) -> Result<Self> {
        super::SolverBase::load_from_huggingface(repo)
            .map_ok(|base| Self { base })
            .await
    }
}

#[async_trait(?Send)]
impl super::super::Solver for Solver {
    async fn solve(
        &self,
        session: &crate::session::Session,
        mut request: super::super::Request,
    ) -> Result<super::super::Response> {
        let Inputs { context, question } = request.parse_multipart().await?;

        let inputs_str: Vec<_> = context
            .iter()
            .map(|context| &context.0)
            .flat_map(|context| {
                question.iter().map(|question| &question.0).map(|question| {
                    super::QuestionWordInput {
                        context: context.clone(),
                        question: question.clone(),
                    }
                })
            })
            .collect();
        if inputs_str.is_empty() {
            let outputs: Outputs = Default::default();
            return super::super::Response::from_json(&outputs);
        }

        let super::TokenizedInputs {
            input_ids,
            inputs,
            inputs_str,
        } = self.base.tokenizer.encode(inputs_str, true)?;

        let inputs: Vec<_> = inputs
            .into_iter()
            .filter_map(|(name, value)| {
                session
                    .inputs()
                    .get(&name)
                    .map(|field| (name, field, value))
            })
            .sorted_by_key(|(_, field, _)| field.index)
            .map(|(name, field, value)| match field.tensor_type {
                TensorType::Int64 => Ok(InputTensor::Int64Tensor(value)),
                _ => bail!("failed to convert tensor type: {name:?}"),
            })
            .collect::<Result<_>>()?;

        let outputs = session.run_raw(&inputs)?;

        let start_logits = outputs.try_extract("start_logits")?;
        let end_logits = outputs.try_extract("end_logits")?;

        let answers = find_answer(&input_ids, &start_logits, &end_logits);

        let outputs: Outputs = inputs_str
            .into_iter()
            .zip(answers.into_iter())
            .map(|(input, answer)| Output {
                input,
                answer: self.base.tokenizer.decode(answer.as_slice().unwrap()),
            })
            .collect();

        super::super::Response::from_json(&outputs)
    }
}

#[derive(MultipartForm)]
struct Inputs {
    context: Vec<Text<String>>,
    question: Vec<Text<String>>,
}

type Outputs = Vec<Output>;

#[derive(Serialize)]
pub struct Output {
    #[serde(flatten)]
    pub input: super::QuestionWordInput,
    pub answer: String,
}

fn find_answer<S, D>(
    mat: &ndarray::ArrayBase<S, D>,
    start_logits: &OutputTensor,
    end_logits: &OutputTensor,
) -> Vec<ndarray::Array1<S::Elem>>
where
    S: ndarray::Data,
    S::Elem: Copy,
    D: ndarray::Dimension,
{
    let start_logits = start_logits.argmax();
    let end_logits = end_logits.argmax();

    mat.rows()
        .into_iter()
        .zip(start_logits)
        .zip(end_logits)
        .map(|((row, start), end)| {
            row.into_iter()
                .skip(start)
                .take(if end >= start { end - start + 1 } else { 1 })
                .copied()
                .collect()
        })
        .collect()
}
