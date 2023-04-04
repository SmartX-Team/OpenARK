use ipis::{async_trait::async_trait, core::anyhow::Result, futures::TryFutureExt};
use netai_api::nlp::question_answering::{Inputs, Outputs};

pub struct Solver {
    base: super::SolverBase,
}

impl Solver {
    pub async fn load_from_huggingface(repo: &str) -> Result<Self> {
        super::SolverBase::load_from_huggingface(repo)
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

        crate::io::Response::from_json::<Outputs>(&outputs)
    }
}
