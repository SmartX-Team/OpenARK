use anyhow::Result;
use dash_api::model::{
    ModelFieldKindNativeSpec, ModelFieldKindStringSpec, ModelFieldNativeSpec, ModelFieldsNativeSpec,
};
use proxyx_api::{client::Client, field::NaturalField};

#[tokio::main]
async fn main() -> Result<()> {
    let mut client = Client::try_default().await?;

    let fields: ModelFieldsNativeSpec = vec![
        ModelFieldNativeSpec {
            name: "/name/".into(),
            kind: ModelFieldKindNativeSpec::String {
                default: None,
                kind: ModelFieldKindStringSpec::Dynamic {},
            },
            attribute: Default::default(),
        },
        ModelFieldNativeSpec {
            name: "/value/".into(),
            kind: ModelFieldKindNativeSpec::Integer {
                default: None,
                minimum: None,
                maximum: None,
            },
            attribute: Default::default(),
        },
    ];
    client.update_fields(fields);

    let value = ::serde_json::json!({
        "name": "Hello world!",
        "value": 42,
    });
    client.add_json(&value).await?;

    client.reload_cache().await?;

    for key in value.as_object().unwrap().keys() {
        let name = format!("/{key}/");
        let value = client.get_raw(&name);
        println!("{name} = {value}");
    }

    let context = "My name is Ho Kim and I live in Gwangju.";
    println!("\nContext: {context}");
    for name in &["/name/"] {
        if let Some(answer) = client.question(name, context).await? {
            let value = &answer.label;
            println!("{name} = {value}");
        }
    }

    // let response = {
    //     // Getting the API key here
    //     let key: String = ::ipis::env::infer("OPENAI_API_KEY")?;

    //     // Creating a new ChatGPT client.
    //     // Note that it requires an API key, and uses
    //     // tokens from your OpenAI API account balance.
    //     let client = ::chatgpt::prelude::ChatGPT::new(key)?;

    //     let field = NaturalField {
    //         native: ModelFieldNativeSpec {
    //             name: "/ramen/".into(),
    //             kind: ModelFieldKindNativeSpec::Object {
    //                 children: Default::default(),
    //                 dynamic: Default::default(),
    //             },
    //             attribute: Default::default(),
    //         },
    //         description: Some("Cup ramen".into()),
    //     };

    //     // Sending a message and getting the completion
    //     let response: ::chatgpt::types::CompletionResponse = client
    //         .send_message(::proxyx_api::prompts::divider::parse(&field))
    //         .await?;

    //     println!("Response: {}", response.message().content);
    //     response.message().content.clone()
    // };
    let response = r#"'/ramen/brand/' as String "What is the brand of the cup ramen?", '/ramen/flavor/' as String "What is the flavor of the cup ramen?", '/ramen/ingredients/' as String[] "What are the ingredients of the cup ramen?", '/ramen/calories/' as Integer "How many calories does the cup ramen have?", '/ramen/sodium/' as Number "What is the sodium content of the cup ramen?", '/ramen/spice-level/' as Integer "On a scale of 1-10, how spicy is the cup ramen?", '/ramen/expiration-date/' as DateTime "When does the cup ramen expire?", '/ramen/origin/' as String "Where was the cup ramen manufactured?", '/ramen/price/' as Number "What is the price of the cup ramen?", '/ramen/nutritional-information/' as Object "Nutritional information of the cup ramen" => '/ramen/nutritional-information/calories/' as Integer "How many calories does the cup ramen have?", '/ramen/nutritional-information/fat/' as Number "What is the fat content of the cup ramen?", '/ramen/nutritional-information/carbohydrates/' as Number "What is the carbohydrate content of the cup ramen?", '/ramen/nutritional-information/protein/' as Number "What is the protein content of the cup ramen?", ..."#;

    {
        #[derive(Debug)]
        struct WeakNaturalField {
            name: String,
            kind: String,
            question: Option<String>,
        }

        // TODO: 질문 수집 -> 빈칸 수집 -> 데이터 수집
        pub const RE: &str = r#"'(?P<name>/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*)' *as *(?P<kind>[a-zA-Z]+) *"(?P<question>[ .,!?a-zA-Z0-9_-]*)",?"#;

        for field in ::regex::Regex::new(RE)?.captures_iter(&response) {
            let name = field["name"].to_string();
            let kind = field["kind"].to_string();
            let question = field["question"].to_string();

            let field = WeakNaturalField {
                name,
                kind,
                question: Some(question),
            };
            dbg!(field);
        }
    }

    Ok(())
}
