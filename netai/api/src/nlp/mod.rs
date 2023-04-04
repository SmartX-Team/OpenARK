pub mod question_answering;
pub mod zero_shot_classification;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct QuestionWordInputs(pub Vec<QuestionWordInput>);

pub type QuestionWordInputsRef<'a> = [QuestionWordInputRef<'a>];

#[cfg(feature = "actix")]
mod impl_multipart_form_for_qustion_word_inputs {
    use actix_multipart::{
        form::{text::Text, Limits, MultipartCollect, MultipartForm, State},
        Field, MultipartError,
    };
    use actix_web::HttpRequest;
    use ipis::futures::future::LocalBoxFuture;

    #[derive(MultipartForm)]
    struct Template {
        context: Vec<Text<String>>,
        question: Vec<Text<String>>,
    }

    impl MultipartCollect for super::QuestionWordInputs {
        fn limit(field_name: &str) -> Option<usize> {
            <Template as MultipartCollect>::limit(field_name)
        }

        fn handle_field<'t>(
            req: &'t HttpRequest,
            field: Field,
            limits: &'t mut Limits,
            state: &'t mut State,
        ) -> LocalBoxFuture<'t, Result<(), MultipartError>> {
            <Template as MultipartCollect>::handle_field(req, field, limits, state)
        }

        fn from_state(state: State) -> Result<Self, MultipartError> {
            <Template as MultipartCollect>::from_state(state).map(
                |Template { context, question }| {
                    let question: Vec<_> =
                        question.into_iter().map(|question| question.0).collect();
                    Self(
                        context
                            .iter()
                            .map(|context| super::QuestionWordInput {
                                context: context.0.clone(),
                                question: question.clone(),
                            })
                            .collect(),
                    )
                },
            )
        }
    }
}

pub type QuestionWordInput = QuestionWord<String, Vec<String>>;
pub type QuestionWordInputRef<'a> = QuestionWord<&'a str, &'a [&'a str]>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct QuestionWord<Context, Question> {
    pub context: Context,
    pub question: Question,
}
