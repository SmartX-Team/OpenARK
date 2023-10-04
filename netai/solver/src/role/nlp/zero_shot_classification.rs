use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::TryFutureExt;
use netai_api::nlp::zero_shot_classification::{Inputs, Outputs};

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
    ) -> Result<Vec<Option<String>>> {
        let Inputs { 0: inputs_str } = inputs;
        if inputs_str.is_empty() {
            return Ok(Default::default());
        }

        let super::LabelToId {
            contradiction,
            entailment,
            neutral,
        } = &self.base.tokenizer.options.label2id;

        // validate labels
        let _ = match *contradiction {
            Some(index) => index as usize,
            None => bail!("'contradiction' label is required"),
        };
        let entailment = match *entailment {
            Some(index) => index as usize,
            None => bail!("'entailment' label is required"),
        };
        let neutral = neutral.map(Into::into);

        let super::TokenizedInputs {
            input_ids: _,
            inputs,
        } = self.base.tokenizer.encode(session, &inputs_str, true)?;

        let raw_outputs = session.run_raw(&inputs)?;

        let logits = raw_outputs.try_extract("logits")?;

        let groups: Vec<_> = inputs_str
            .iter()
            .map(|input| input.question.len())
            .collect();
        let answers = logits.argmax_by_group(entailment, neutral, &groups);

        let outputs = answers
            .into_iter()
            .zip(&inputs_str)
            .map(|(index, input)| index.map(|index| input.question[index].to_string()))
            .collect();
        Ok(outputs)
    }
}
