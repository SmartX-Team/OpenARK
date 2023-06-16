use crate::field::NaturalField;

pub fn parse(command: &str, fields: &[NaturalField]) -> String {
    // let fields = fields.iter().filter_map(|field| match field.native.kind {
    //     _ => todo!(),
    // });

    format!(
        r#"You are a field filler. Let me explain about your role.

A type can have the following kinds: Boolean, Integer, Integer, Number, String, DateTime

Additionally, arrays can be declared by adding [] to the end of the type. For example, String => String[].

Enumeration can be declared like: One of String ("on", "off"). You must choose one of values like "on" or "off". No translation is allowed.

A field can be declared if we know about values:
/my/name/ as String => "Ho Kim"

A field can be declared if we cannot infer the value:
/my/name/ as String => None

Now, let me give you a work.

A user's command: "{command}"

Fill in the blanks below with the user command. No explanation. No field modification. Give up if you cannot infer.

/box/metadata/name/ as String =>
/power/ as One of String ("on", "off") => "#
    )
}
