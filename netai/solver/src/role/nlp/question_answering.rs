use anyhow::Result;
use async_trait::async_trait;
use futures::TryFutureExt;
use netai_api::nlp::question_answering::{Inputs, Outputs};

use crate::tensor::BatchedTensor;

pub struct Solver {
    base: super::SolverBase,
}

impl Solver {
    pub async fn load_from_huggingface(role: super::Role, repo: &str) -> Result<Self> {
        super::SolverBase::load_from_huggingface(role, repo)
            .map_ok(|base| Self { base })
            .await
    }
}

#[async_trait(?Send)]
impl crate::Solver for Solver {
    async fn solve(
        &self,
        _session: &crate::session::Session,
        _tensors: BatchedTensor,
    ) -> Result<BatchedTensor> {
        todo!()
    }

    async fn solve_web(
        &self,
        session: &crate::session::Session,
        mut request: crate::io::Request,
    ) -> Result<crate::io::Response> {
        let inputs = request.parse().await?;
        let outputs = self.solve_raw(session, inputs)?;
        crate::io::Response::from_json::<Outputs>(&outputs)
    }
}

impl Solver {
    fn solve_raw(
        &self,
        session: &crate::session::Session,
        inputs: Inputs,
    ) -> Result<Vec<Vec<String>>> {
        let Inputs { 0: inputs_str } = inputs;
        if inputs_str.is_empty() {
            return Ok(Default::default());
        }

        let super::TokenizedInputs { input_ids, inputs } =
            self.base.tokenizer.encode(session, &inputs_str, true)?;

        let raw_outputs = session.run_raw(&inputs)?;

        let start_logits = raw_outputs.try_extract("start_logits")?.argmax();
        let end_logits = raw_outputs.try_extract("end_logits")?.argmax();

        let mut answers = input_ids
            .rows()
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
            .map(|answer: Vec<_>| self.base.tokenizer.decode(&answer));

        let outputs = inputs_str
            .iter()
            .map(|input| {
                answers
                    .by_ref()
                    .take(input.question.len())
                    .collect::<Vec<_>>()
            })
            .collect();
        Ok(outputs)
    }
}
