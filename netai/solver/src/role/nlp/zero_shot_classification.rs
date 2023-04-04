use ipis::{
    async_trait::async_trait,
    core::anyhow::{bail, Result},
    futures::TryFutureExt,
};
use netai_api::nlp::zero_shot_classification::{Inputs, Outputs};

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
        session: &crate::session::Session,
        mut request: crate::io::Request,
    ) -> Result<crate::io::Response> {
        let Inputs { 0: inputs_str } = request.parse().await?;

        if inputs_str.is_empty() {
            let outputs: Outputs = Default::default();
            return crate::io::Response::from_json(&outputs);
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
            .map(|(index, input)| index.map(|index| input.question[index].as_str()))
            .collect();

        crate::io::Response::from_json::<Outputs>(&outputs)
    }
}
