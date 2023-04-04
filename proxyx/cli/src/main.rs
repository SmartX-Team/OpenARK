use dash_api::model::{
    ModelFieldKindNativeSpec, ModelFieldKindStringSpec, ModelFieldNativeSpec, ModelFieldsNativeSpec,
};
use ipis::{core::anyhow::Result, tokio};
use proxyx_api::client::Client;

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
    Ok(())
}
