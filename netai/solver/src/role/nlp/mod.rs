pub mod question_answering;
pub mod zero_shot_classification;

use std::borrow::Cow;

use ipis::{
    core::{
        anyhow::{bail, Result},
        ndarray,
    },
    futures::TryFutureExt,
    itertools::Itertools,
};
use netai_api::nlp::{QuestionWord, QuestionWordInput};
use ort::tensor::InputTensor;
use rust_tokenizers::{tokenizer::TruncationStrategy, TokenizedInput};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

pub struct SolverBase {
    tokenizer: Tokenizer,
}

impl SolverBase {
    async fn load_from_huggingface(role: Role, repo: &str) -> Result<Self> {
        Tokenizer::load_from_huggingface(role, repo)
            .map_ok(|tokenizer| Self { tokenizer })
            .await
    }
}

struct Tokenizer {
    base: TokenizerBase,
    options: TokenizerOptions,
    order: TokenizeOrder,
    role: Role,
}

impl Tokenizer {
    async fn load_from_huggingface(role: Role, repo: &str) -> Result<Self> {
        use crate::models::huggingface as model;

        #[derive(Default, Deserialize)]
        struct Config {
            #[serde(default)]
            model_type: Option<String>,
            #[serde(default, flatten)]
            options: TokenizerOptions,
        }

        #[derive(Default, Deserialize)]
        struct TokenizerConfig {
            #[serde(default)]
            add_prefix_space: bool,
            #[serde(default)]
            do_lower_case: bool,
            #[serde(default)]
            strip_accents: bool,
        }

        let Config {
            model_type,
            options,
        } = model::get_json(repo, "config.json").await?;

        let TokenizerConfig {
            add_prefix_space,
            do_lower_case: lower_case,
            strip_accents,
        } = model::get_json(repo, "tokenizer_config.json").await?;

        let base = match model_type.as_deref() {
            Some("distilbert") => {
                let vocab_path = model::get_file(repo, "vocab.txt").await?;

                ::rust_tokenizers::tokenizer::DeBERTaV2Tokenizer::from_file(
                    vocab_path,
                    lower_case,
                    strip_accents,
                    add_prefix_space,
                )
                .map(TokenizerBase::DeBERTaV2)?
            }
            Some("bart") | Some("roberta") => {
                let vocab_path = model::get_file(repo, "vocab.json").await?;
                let merges_path = model::get_file(repo, "merges.txt").await?;

                ::rust_tokenizers::tokenizer::RobertaTokenizer::from_file(
                    vocab_path,
                    merges_path,
                    lower_case,
                    add_prefix_space,
                )
                .map(TokenizerBase::Roberta)?
            }
            Some(model_type) => bail!("unsupported model type: {model_type:?}"),
            None => bail!("cannot infer a dynamic model type"),
        };

        let order = match role {
            Role::QuestionAnswering => TokenizeOrder::QuestionFirst,
            Role::ZeroShotClassification => TokenizeOrder::ContextFirst,
        };

        Ok(Self {
            base,
            options,
            order,
            role,
        })
    }

    fn encode<Inputs>(
        &self,
        session: &crate::session::Session,
        inputs_str: Inputs,
        to_tensor: bool,
    ) -> Result<TokenizedInputs>
    where
        Inputs: CollectTokenizerInputs,
    {
        match &self.base {
            TokenizerBase::DeBERTaV2(tokenizer) => {
                self.encode_with(session, tokenizer, inputs_str, to_tensor)
            }
            TokenizerBase::Roberta(tokenizer) => {
                self.encode_with(session, tokenizer, inputs_str, to_tensor)
            }
        }
    }

    fn encode_with<Inputs, T, V>(
        &self,
        session: &crate::session::Session,
        tokenizer: &T,
        inputs_str: Inputs,
        to_tensor: bool,
    ) -> Result<TokenizedInputs>
    where
        Inputs: CollectTokenizerInputs,
        T: ::rust_tokenizers::tokenizer::Tokenizer<V>,
        V: ::rust_tokenizers::vocab::Vocab,
    {
        fn collect_encode_batch<'a, T, TIter>(
            encodings: &'a [TokenizedInput],
            max_len: usize,
            pad: i64,
            f: impl Fn(&'a TokenizedInput) -> TIter,
        ) -> ::ipis::core::anyhow::Result<ndarray::Array<i64, ndarray::Ix2>>
        where
            T: 'a + Copy + Into<i64>,
            TIter: IntoIterator<Item = T>,
        {
            let arrays: Vec<_> = encodings
                .iter()
                .map(|encoding| f(encoding).into_iter().map(Into::into).collect::<Vec<_>>())
                .map(|mut input| {
                    input.extend([pad].repeat(max_len - input.len()));
                    input
                })
                .map(ndarray::Array::from)
                .map(|input| {
                    let length = input.len();
                    input.into_shape((1, length))
                })
                .collect::<Result<_, _>>()?;

            let arrays: Vec<_> = arrays.iter().map(|array| array.view()).collect();
            ndarray::concatenate(ndarray::Axis(0), &arrays).map_err(Into::into)
        }

        let inputs_str_raw = inputs_str.collect_tokenizer_inputs(&self.order);

        let max_len = inputs_str_raw
            .iter()
            .map(|TokenizerInput { text_1, text_2 }| {
                text_1.len().max(text_2.map(|e| e.len()).unwrap_or(0))
            })
            .max()
            .unwrap_or(0);

        let inputs_1: Vec<_> = inputs_str_raw
            .iter()
            .map(|TokenizerInput { text_1, text_2: _ }| text_1)
            .collect();
        let inputs_2: Vec<_> = inputs_str_raw
            .iter()
            .filter_map(|TokenizerInput { text_1: _, text_2 }| text_2.as_ref())
            .collect();

        if !inputs_2.is_empty() && inputs_1.len() != inputs_2.len() {
            bail!("failed to parse the text pairs");
        }

        let encodings = if inputs_2.is_empty() {
            tokenizer.encode_list(&inputs_1, max_len, &TruncationStrategy::LongestFirst, 0)
        } else {
            let inputs_pair: Vec<_> = inputs_1
                .iter()
                .map(|&&text| Cow::Borrowed(text))
                .zip(inputs_2.iter().map(|&&text| match self.role {
                    Role::QuestionAnswering => Cow::Borrowed(text),
                    Role::ZeroShotClassification => Cow::Owned(format!("This example is {text}.")),
                }))
                .collect();

            tokenizer.encode_pair_list(&inputs_pair, max_len, &TruncationStrategy::LongestFirst, 0)
        };
        let input_lens: Vec<_> = encodings
            .iter()
            .map(|encoding| encoding.token_ids.len())
            .collect();
        let max_len = input_lens.iter().max().copied().unwrap_or(0);

        let input_ids_pad = self.options.pad_token_id;
        let input_ids = collect_encode_batch(&encodings, max_len, input_ids_pad, |encoding| {
            encoding.token_ids.iter().copied()
        })?;

        let inputs = if to_tensor {
            let attention_mask_pad = 0;
            let attention_mask =
                collect_encode_batch(&encodings, max_len, attention_mask_pad, |encoding| {
                    vec![1; encoding.token_ids.len()]
                })?;

            let token_type_ids_pad = 0;
            let token_type_ids =
                collect_encode_batch(&encodings, max_len, token_type_ids_pad, |encoding| {
                    encoding.segment_ids.iter().copied()
                })?;

            vec![
                ("attention_mask".to_string(), attention_mask.into_dyn()),
                ("input_ids".to_string(), input_ids.clone().into_dyn()),
                ("token_type_ids".to_string(), token_type_ids.into_dyn()),
            ]
            .into_iter()
            .filter_map(|(name, value)| {
                session
                    .inputs()
                    .get(&name)
                    .map(|field| (name, field, value))
            })
            .sorted_by_key(|(_, field, _)| field.index)
            .map(|(name, field, value)| match field.tensor_type {
                crate::tensor::TensorType::Int64 => Ok(InputTensor::Int64Tensor(value)),
                _ => bail!("failed to convert tensor type: {name:?}"),
            })
            .collect::<Result<_>>()?
        } else {
            Default::default()
        };

        Ok(TokenizedInputs { input_ids, inputs })
    }

    fn decode(&self, token_ids: &[i64]) -> String {
        let skip_special_tokens = true;
        let clean_up_tokenization_spaces = true;

        match &self.base {
            TokenizerBase::DeBERTaV2(tokenizer) => Self::decode_with(
                tokenizer,
                token_ids,
                skip_special_tokens,
                clean_up_tokenization_spaces,
            ),
            TokenizerBase::Roberta(tokenizer) => Self::decode_with(
                tokenizer,
                token_ids,
                skip_special_tokens,
                clean_up_tokenization_spaces,
            ),
        }
    }

    fn decode_with<T, V>(
        tokenizer: &T,
        token_ids: &[i64],
        skip_special_tokens: bool,
        clean_up_tokenization_spaces: bool,
    ) -> String
    where
        T: ::rust_tokenizers::tokenizer::Tokenizer<V>,
        V: ::rust_tokenizers::vocab::Vocab,
    {
        tokenizer
            .decode(token_ids, skip_special_tokens, clean_up_tokenization_spaces)
            .trim()
            .to_string()
    }
}

enum TokenizerBase {
    DeBERTaV2(::rust_tokenizers::tokenizer::DeBERTaV2Tokenizer),
    Roberta(::rust_tokenizers::tokenizer::RobertaTokenizer),
}

#[derive(Copy, Clone, Debug, Default, Deserialize)]
struct TokenizerOptions {
    #[serde(default)]
    label2id: LabelToId,
    #[serde(default)]
    pad_token_id: i64,
}

#[derive(Copy, Clone, Debug, Default, Deserialize)]
struct LabelToId {
    #[serde(default)]
    contradiction: Option<u8>,
    #[serde(default)]
    entailment: Option<u8>,
    #[serde(default)]
    neutral: Option<u8>,
}

impl CollectTokenizerInputs for Vec<QuestionWordInput> {
    fn collect_tokenizer_inputs(&self, order: &TokenizeOrder) -> TokenizerInputs<'_> {
        self.iter()
            .flat_map(|QuestionWord { context, question }| {
                question.iter().map(|question| match order {
                    TokenizeOrder::ContextFirst => TokenizerInput {
                        text_1: context,
                        text_2: Some(question),
                    },
                    TokenizeOrder::QuestionFirst => TokenizerInput {
                        text_1: question,
                        text_2: Some(context),
                    },
                })
            })
            .collect()
    }
}
trait CollectTokenizerInputs {
    fn collect_tokenizer_inputs(&self, order: &TokenizeOrder) -> TokenizerInputs<'_>;
}

impl<T> CollectTokenizerInputs for &T
where
    T: CollectTokenizerInputs,
{
    fn collect_tokenizer_inputs(&self, order: &TokenizeOrder) -> TokenizerInputs<'_> {
        (**self).collect_tokenizer_inputs(order)
    }
}

type TokenizerInputs<'a> = Vec<TokenizerInput<'a>>;

struct TokenizerInput<'a> {
    text_1: &'a str,
    text_2: Option<&'a str>,
}

struct TokenizedInputs {
    input_ids: ndarray::Array<i64, ndarray::Ix2>,
    inputs: Vec<InputTensor>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TokenizeOrder {
    ContextFirst,
    QuestionFirst,
}

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
    pub(super) const fn as_huggingface_feature(&self) -> &'static str {
        match self {
            Self::QuestionAnswering => "question-answering",
            Self::ZeroShotClassification => "sequence-classification",
        }
    }

    pub(super) async fn load_solver(
        &self,
        model: impl crate::models::Model,
    ) -> Result<crate::BoxSolver> {
        match self {
            // NLP
            Self::QuestionAnswering => match model.get_kind() {
                crate::models::ModelKind::Huggingface => {
                    crate::role::nlp::question_answering::Solver::load_from_huggingface(
                        *self,
                        &model.get_repo(),
                    )
                    .await
                    .map(|solver| Box::new(solver) as crate::BoxSolver)
                }
            },
            Self::ZeroShotClassification => match model.get_kind() {
                crate::models::ModelKind::Huggingface => {
                    crate::role::nlp::zero_shot_classification::Solver::load_from_huggingface(
                        *self,
                        &model.get_repo(),
                    )
                    .await
                    .map(|solver| Box::new(solver) as crate::BoxSolver)
                }
            },
        }
    }
}