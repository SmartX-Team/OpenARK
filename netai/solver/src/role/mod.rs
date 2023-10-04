mod nlp;

use std::str::FromStr;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use strum::{Display, ParseError};

#[derive(
    Copy, Clone, Debug, Display, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub enum Role {
    Nlp(self::nlp::Role),
}

impl FromStr for Role {
    type Err = ParseError;

    fn from_str(role: &str) -> std::result::Result<Self, Self::Err> {
        <self::nlp::Role as FromStr>::from_str(role).map(Self::Nlp)
    }
}

#[cfg(feature = "clap")]
impl ::clap::ValueEnum for Role {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Nlp(self::nlp::Role::QuestionAnswering),
            Self::Nlp(self::nlp::Role::ZeroShotClassification),
        ]
    }

    fn to_possible_value(&self) -> Option<::clap::builder::PossibleValue> {
        match self {
            Role::Nlp(value) => value.to_possible_value(),
        }
    }
}

impl Role {
    pub(super) const fn as_huggingface_feature(&self) -> &'static str {
        match self {
            Self::Nlp(role) => role.as_huggingface_feature(),
        }
    }

    pub(super) async fn load_solver(
        &self,
        model: impl crate::models::Model,
    ) -> Result<crate::BoxSolver> {
        match self {
            // NLP
            Self::Nlp(role) => role.load_solver(model).await,
        }
    }
}
