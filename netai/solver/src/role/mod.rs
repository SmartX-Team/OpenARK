pub mod nlp;

use ipis::core::anyhow::Result;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{
    models::{Model, ModelKind},
    BoxSolver,
};

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum Role {
    // NLP
    QuestionAnswering,
    ZeroShotClassification,
}

impl Role {
    pub(crate) const fn as_huggingface_feature(&self) -> &'static str {
        match self {
            Self::QuestionAnswering => "question-answering",
            Self::ZeroShotClassification => "sequence-classification",
        }
    }

    pub(crate) async fn load_solver(&self, model: impl Model) -> Result<BoxSolver> {
        match self {
            // NLP
            Self::QuestionAnswering => match model.get_kind() {
                ModelKind::Huggingface => {
                    crate::role::nlp::question_answering::Solver::load_from_huggingface(
                        &model.get_repo(),
                    )
                    .await
                    .map(|solver| Box::new(solver) as BoxSolver)
                }
            },
            Self::ZeroShotClassification => match model.get_kind() {
                ModelKind::Huggingface => {
                    crate::role::nlp::zero_shot_classification::Solver::load_from_huggingface(
                        &model.get_repo(),
                    )
                    .await
                    .map(|solver| Box::new(solver) as BoxSolver)
                }
            },
        }
    }
}
