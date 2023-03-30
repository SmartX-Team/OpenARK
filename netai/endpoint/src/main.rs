use std::{collections::BTreeMap, path::PathBuf};

use ipis::{
    core::{
        anyhow::{bail, Result},
        ndarray,
    },
    futures::StreamExt,
    tokio,
};
use netai_api::{models::huggingface::HuggingfaceModel, role::Role, session::Session};
use ort::tensor::InputTensor;
use rust_tokenizers::{
    tokenizer::{Tokenizer, TruncationStrategy},
    TokenizedInput,
};
use serde::{de::DeserializeOwned, Deserialize};

#[tokio::main]
async fn main() -> Result<()> {
    let model = HuggingfaceModel {
        repo: "deepset/roberta-base-squad2".into(),
        role: Role::QuestionAnswering,
    };

    let session = Session::try_new(model).await?;
    dbg!(session.inputs());
    dbg!(session.outputs());

    async fn get_file(repo: &str, name: &str) -> Result<PathBuf> {
        let url = format!("https://huggingface.co/{repo}/raw/main/{name}");
        let response = reqwest::get(&url).await?;
        if !response.status().is_success() {
            let status = response.status();
            bail!("failed to download file: [{status}] {url}");
        }
        let mut byte_stream = response.bytes_stream();

        let path: PathBuf = format!("/models/{name}").parse()?;
        let mut file = tokio::fs::File::create(&path).await?;

        while let Some(item) = byte_stream.next().await {
            tokio::io::copy(&mut item?.as_ref(), &mut file).await?;
        }
        Ok(path)
    }

    async fn get_json<T>(repo: &str, name: &str) -> Result<T>
    where
        T: Default + DeserializeOwned,
    {
        let url = format!("https://huggingface.co/{repo}/raw/main/{name}");
        reqwest::get(url).await?.json().await.map_err(Into::into)
    }

    enum Tokenizer {
        DeBERTaV2(::rust_tokenizers::tokenizer::DeBERTaV2Tokenizer),
        Roberta(::rust_tokenizers::tokenizer::RobertaTokenizer),
    }

    let tokenizer = {
        #[derive(Default, Deserialize)]
        struct Config {
            model_type: Option<String>,
        }

        #[derive(Default, Deserialize)]
        struct TokenizerConfig {
            add_prefix_space: Option<bool>,
            do_lower_case: Option<bool>,
            strip_accents: Option<bool>,
        }

        let config: Config = get_json("deepset/roberta-base-squad2", "config.json").await?;

        match config.model_type.as_deref() {
            Some("distilbert") => {
                let vocab_path = get_file("deepset/roberta-base-squad2", "vocab.txt").await?;

                let config: TokenizerConfig =
                    get_json("deepset/roberta-base-squad2", "tokenizer_config.json").await?;

                ::rust_tokenizers::tokenizer::DeBERTaV2Tokenizer::from_file(
                    vocab_path,
                    config.do_lower_case.unwrap_or_default(),
                    config.strip_accents.unwrap_or_default(),
                    config.add_prefix_space.unwrap_or_default(),
                )
                .map(Tokenizer::DeBERTaV2)?
            }
            Some("roberta") => {
                let vocab_path = get_file("deepset/roberta-base-squad2", "vocab.json").await?;
                let merges_path = get_file("deepset/roberta-base-squad2", "merges.txt").await?;

                let config: TokenizerConfig =
                    get_json("deepset/roberta-base-squad2", "tokenizer_config.json").await?;

                ::rust_tokenizers::tokenizer::RobertaTokenizer::from_file(
                    vocab_path,
                    merges_path,
                    config.do_lower_case.unwrap_or_default(),
                    config.add_prefix_space.unwrap_or_default(),
                )
                .map(Tokenizer::Roberta)?
            }
            Some(model_type) => bail!("unsupported model type: {model_type:?}"),
            None => bail!("cannot infer a dynamic model type"),
        }
    };

    let input_strs = vec![GenericInput {
        text_1: "How much apples?".into(),
        text_2: Some("There are 3 apples.".into()),
    }];
    let mut encodings = match &tokenizer {
        Tokenizer::DeBERTaV2(tokenizer) => tokenize(tokenizer, input_strs.clone(), true)?,
        Tokenizer::Roberta(tokenizer) => tokenize(tokenizer, input_strs.clone(), true)?,
    };
    println!("{:?}", &encodings.inputs_str);

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

    for (input, answer) in input_strs.into_iter().zip(answers.into_iter()) {
        dbg!(&answer);
        let answer = match &tokenizer {
            Tokenizer::DeBERTaV2(tokenizer) => {
                tokenizer.decode(answer.as_slice().unwrap(), true, true)
            }
            Tokenizer::Roberta(tokenizer) => {
                tokenizer.decode(answer.as_slice().unwrap(), true, true)
            }
        }
        .trim()
        .to_string();
        println!("{} = {answer}", &input.text_1);
    }

    Ok(())
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
    let start_logits = argmax(start_logits);
    let end_logits = argmax(end_logits);
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

fn argmax<S, D>(mat: &ndarray::ArrayBase<S, D>) -> ndarray::Array1<usize>
where
    S: ndarray::Data,
    S::Elem: PartialOrd,
    D: ndarray::Dimension,
{
    mat.rows()
        .into_iter()
        .map(|row| {
            row.into_iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                .unwrap()
                .0
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenericInput {
    pub text_1: String,
    pub text_2: Option<String>,
}

pub struct Tokenized {
    pub input_ids: ndarray::Array<i64, ndarray::Ix2>,
    pub inputs: BTreeMap<String, InputTensor>,
    pub inputs_str: Vec<GenericInput>,
}

fn tokenize<T, V>(
    tokenizer: &T,
    inputs_str: Vec<GenericInput>,
    to_tensor: bool,
) -> ::ipis::core::anyhow::Result<Tokenized>
where
    T: ::rust_tokenizers::tokenizer::Tokenizer<V>,
    V: ::rust_tokenizers::vocab::Vocab,
{
    fn collect_encode_batch<T>(
        encodings: &[TokenizedInput],
        max_len: usize,
        f: impl Fn(&TokenizedInput) -> &[T],
    ) -> ::ipis::core::anyhow::Result<ndarray::Array<i64, ndarray::Ix2>>
    where
        T: Copy + Into<i64>,
    {
        let arrays: Vec<_> = encodings
            .iter()
            .map(|encoding| {
                f(encoding)
                    .iter()
                    .copied()
                    .map(Into::into)
                    .collect::<Vec<_>>()
            })
            .map(|mut input| {
                input.extend([0].repeat(max_len - input.len()));
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

    let max_len = inputs_str
        .iter()
        .map(|input| {
            input
                .text_1
                .len()
                .max(input.text_2.as_ref().map(|e| e.len()).unwrap_or(0))
        })
        .max()
        .unwrap_or(0);

    let inputs_1: Vec<_> = inputs_str
        .iter()
        .map(|input| input.text_1.as_str())
        .collect();
    let inputs_2: Vec<_> = inputs_str
        .iter()
        .filter_map(|input| input.text_2.as_deref())
        .collect();

    if !inputs_2.is_empty() && inputs_1.len() != inputs_2.len() {
        bail!("failed to parse the text pairs");
    }

    let encodings = if inputs_2.is_empty() {
        tokenizer.encode_list(&inputs_1, max_len, &TruncationStrategy::LongestFirst, 0)
    } else {
        let inputs_pair: Vec<_> = inputs_1.into_iter().zip(inputs_2.into_iter()).collect();

        tokenizer.encode_pair_list(&inputs_pair, max_len, &TruncationStrategy::LongestFirst, 0)
    };
    let input_lens: Vec<_> = encodings
        .iter()
        .map(|encoding| encoding.token_ids.len())
        .collect();
    let max_len = input_lens.iter().max().copied().unwrap_or(0);

    let input_ids = collect_encode_batch(&encodings, max_len, |encoding| &encoding.token_ids)?;

    let inputs = if to_tensor {
        let attention_mask = ndarray::Array::ones(input_ids.dim());
        let token_type_ids =
            collect_encode_batch(&encodings, max_len, |encoding| &encoding.segment_ids)?;

        vec![
            (
                "input_ids".to_string(),
                InputTensor::Int64Tensor(input_ids.clone().into_dyn()),
            ),
            (
                "attention_mask".to_string(),
                InputTensor::Int64Tensor(attention_mask.into_dyn()),
            ),
            (
                "token_type_ids".to_string(),
                InputTensor::Int64Tensor(token_type_ids.into_dyn()),
            ),
        ]
        .into_iter()
        .collect()
    } else {
        Default::default()
    };

    Ok(Tokenized {
        input_ids,
        inputs,
        inputs_str,
    })
}
