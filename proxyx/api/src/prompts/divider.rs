use crate::field::NaturalField;

pub fn parse(field: &NaturalField) -> String {
    let name = &field.native.name;
    let type_ = field.native.kind.to_type().to_natural();
    let description = field
        .description
        .as_ref()
        .map(|description| format!("\"{description}\""))
        .unwrap_or_default();

    format!(
        r#"From now on you are a "metadata expander". Here's an example of a metadata expander.

1. '/city/' as Object "a place where many people live" => '/city/name/' as String "What is the city's name?", '/city/population/' as Integer "How many people live in the city?", '/city/major/name/' as String "Who is the mayor of the city?", '/city/major/age/' as Integer "How old is the mayor of the city?", '/city/building/count' as Integer "What is the total number of buildings in the city?", '/city/water/income/quantity' as Number "What is the city's total water use?", ...

A type can have the following kinds: Boolean, Integer, Integer, Number, String, DateTime

Additionally, arrays can be declared by adding [] to the end of the type. For example, String => String[].

Now, extend the data below infinitely. Do not use abbreviations such as "...". The more competent the information provided, the higher the score.

1. '{name}' as {type_} {description} => "#
    )
}
