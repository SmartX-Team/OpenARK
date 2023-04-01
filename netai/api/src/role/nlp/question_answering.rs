use actix_multipart::form::{text::Text, MultipartForm};
use ipis::{
    async_trait::async_trait,
    core::{anyhow::Result, ndarray},
    futures::TryFutureExt,
};
use serde::Serialize;

use crate::ops;

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

        let inputs_str = context
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

        let mut encodings = self.base.tokenizer.encode(inputs_str, true)?;

        // TODO: to be implemented
        let attention_mask = encodings.inputs.remove("attention_mask").unwrap();
        let input_ids = encodings.inputs.remove("input_ids").unwrap();

        let outputs = session.run_raw(&[input_ids, attention_mask])?;

        let start_logits = outputs[0].try_extract::<f32>()?;
        let end_logits = outputs[1].try_extract::<f32>()?;

        let answers = find_answer(
            &encodings.input_ids,
            &start_logits.view(),
            &end_logits.view(),
        );

        let outputs: Outputs = encodings
            .inputs_str
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

fn find_answer<SM, SL, DM, DL>(
    mat: &ndarray::ArrayBase<SM, DM>,
    start_logits: &ndarray::ArrayBase<SL, DL>,
    end_logits: &ndarray::ArrayBase<SL, DL>,
) -> Vec<ndarray::Array1<SM::Elem>>
where
    SM: ndarray::Data,
    SM::Elem: Copy,
    SL: ndarray::Data,
    SL::Elem: Copy + PartialOrd,
    DM: ndarray::Dimension,
    DL: ndarray::Dimension,
    i64: TryFrom<<SM as ndarray::RawData>::Elem>,
    <i64 as TryFrom<<SM as ndarray::RawData>::Elem>>::Error: ::core::fmt::Debug,
{
    let start_logits = ops::argmax(start_logits);
    let end_logits = ops::argmax(end_logits);
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
