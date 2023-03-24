use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use dash_actor::{imp::assert_contains, name, storage::KubernetesStorageClient};
use dash_api::model::{
    ModelCustomResourceDefinitionRefSpec, ModelFieldAttributeSpec, ModelFieldKindExtendedSpec,
    ModelFieldKindNativeSpec, ModelFieldKindSpec, ModelFieldKindStringSpec, ModelFieldNativeSpec,
    ModelFieldSpec, ModelFieldsNativeSpec, ModelFieldsSpec, ModelSpec,
};
use inflector::Inflector;
use ipis::{
    core::anyhow::{bail, Result},
    itertools::Itertools,
    log::warn,
};
use kiss_api::k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::{
    CustomResourceDefinitionVersion, JSONSchemaProps,
};
use regex::Regex;

pub struct ModelValidator<'a> {
    pub kubernetes_storage: KubernetesStorageClient<'a>,
}

impl<'a> ModelValidator<'a> {
    pub async fn validate_model(&self, spec: ModelSpec) -> Result<ModelFieldsNativeSpec> {
        match spec {
            ModelSpec::Fields(spec) => self.validate_fields(spec).await,
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.validate_custom_resource_definition_ref(spec).await
            }
        }
    }

    pub async fn validate_fields(&self, spec: ModelFieldsSpec) -> Result<ModelFieldsNativeSpec> {
        let mut parser = ModelFieldsParser::default();
        for field in spec {
            match field.kind {
                ModelFieldKindSpec::Native(_) => parser.parse_field(field.try_into_native()?)?,
                ModelFieldKindSpec::Extended(kind) => match kind {
                    // BEGIN reference types
                    ModelFieldKindExtendedSpec::Model { name } => self
                        .kubernetes_storage
                        .load_model(&name)
                        .await
                        .and_then(|model| {
                            parser.merge_fields(&field.name, model.into_fields_unchecked())
                        })?,
                },
            }
        }

        parser.finalize()
    }

    pub fn validate_native_fields(
        &self,
        spec: ModelFieldsNativeSpec,
    ) -> Result<ModelFieldsNativeSpec> {
        let mut parser = ModelFieldsParser::default();
        for field in spec {
            parser.parse_field(field)?;
        }

        parser.finalize()
    }

    async fn validate_custom_resource_definition_ref(
        &self,
        spec: ModelCustomResourceDefinitionRefSpec,
    ) -> Result<ModelFieldsNativeSpec> {
        let (_, _, def) = self
            .kubernetes_storage
            .load_custom_resource_definition(&spec)
            .await?;

        let mut parser = ModelFieldsParser::default();
        parser.parse_custom_resource_definition(&def)?;
        self.validate_native_fields(parser.finalize()?)
    }
}

type ModelFieldsNativeMap = BTreeMap<String, ModelFieldNativeSpec>;

#[derive(Debug, Default)]
struct ModelFieldsParser {
    map: ModelFieldsNativeMap,
}

impl ModelFieldsParser {
    fn parse_custom_resource_definition(
        &mut self,
        def: &CustomResourceDefinitionVersion,
    ) -> Result<()> {
        match def
            .schema
            .as_ref()
            .and_then(|schema| schema.open_api_v3_schema.as_ref())
        {
            Some(prop) => self.parse_json_property(None, "", prop).map(|_| ()),
            None => Ok(()),
        }
    }

    fn parse_json_property(
        &mut self,
        parent: Option<&str>,
        name: &str,
        prop: &JSONSchemaProps,
    ) -> Result<String> {
        let (name, name_raw) = (convert_name(parent, name)?, name);

        let kind = match prop.type_.as_ref().map(AsRef::as_ref) {
            // BEGIN primitive types
            Some("boolean") => {
                let default = prop.default.as_ref().and_then(|e| e.0.as_bool());

                Some(ModelFieldKindNativeSpec::Boolean { default })
            }
            Some("integer") => {
                let default = prop.default.as_ref().and_then(|e| e.0.as_i64());
                let minimum = prop.minimum.as_ref().copied().map(|e| e.round() as i64);
                let maximum = prop.maximum.as_ref().copied().map(|e| e.round() as i64);

                Some(ModelFieldKindNativeSpec::Integer {
                    default,
                    minimum,
                    maximum,
                })
            }
            Some("number") => {
                let default = prop.default.as_ref().and_then(|e| e.0.as_f64());
                let minimum = prop.minimum;
                let maximum = prop.maximum;

                Some(ModelFieldKindNativeSpec::Number {
                    default,
                    minimum,
                    maximum,
                })
            }
            Some("string") => match prop.format.as_ref().map(AsRef::as_ref) {
                // BEGIN string format
                Some("date-time") => Some(ModelFieldKindNativeSpec::DateTime { default: None }),
                Some("ip") => Some(ModelFieldKindNativeSpec::Ip {}),
                Some("uuid") => Some(ModelFieldKindNativeSpec::Uuid {}),
                // END string format
                None => match &prop.enum_ {
                    Some(enum_) => {
                        let default = prop
                            .default
                            .as_ref()
                            .and_then(|e| e.0.as_str())
                            .map(ToString::to_string);
                        let choices: BTreeSet<_> = enum_
                            .iter()
                            .filter_map(|e| e.0.as_str())
                            .map(ToString::to_string)
                            .collect();

                        Some(ModelFieldKindNativeSpec::OneOfStrings {
                            default,
                            choices: choices.into_iter().collect(),
                        })
                    }
                    None => {
                        let default = prop
                            .default
                            .as_ref()
                            .and_then(|e| e.0.as_str())
                            .map(ToString::to_string);

                        // TODO: to be implemented
                        let kind = Default::default();

                        Some(ModelFieldKindNativeSpec::String { default, kind })
                    }
                },
                Some(format) => bail!("unknown string format of {name:?}: {format:?}"),
            },
            // BEGIN aggregation types
            Some("object") => {
                let children =
                    self.parse_json_property_aggregation(&name, prop.properties.as_ref())?;

                Some(ModelFieldKindNativeSpec::Object {
                    children: children.into_iter().collect(),
                    dynamic: false,
                })
            }
            Some("array") => {
                // TODO: parse other primitive types
                warn!("Array type only supports for ObjectArray: {name:?}");
                let children =
                    self.parse_json_property_aggregation(&name, prop.properties.as_ref())?;

                Some(ModelFieldKindNativeSpec::ObjectArray {
                    children: children.into_iter().collect(),
                })
            }
            type_ => bail!("unknown type of {name:?}: {type_:?}"),
        };

        match kind {
            Some(kind) => {
                let spec = ModelFieldSpec {
                    name: name.clone(),
                    kind,
                    attribute: ModelFieldAttributeSpec {
                        optional: prop.nullable.unwrap_or_default(),
                    },
                };

                self.insert_field(name.clone(), name_raw, spec)
                    .map(|()| name)
            }
            None => Ok(name),
        }
    }

    fn insert_field(
        &mut self,
        name: String,
        name_raw: &str,
        spec: ModelFieldNativeSpec,
    ) -> Result<()> {
        match self.map.insert(name.clone(), spec) {
            None => Ok(()),
            Some(_) => bail!("conflicted field name: {name} ({name_raw})"),
        }
    }

    fn parse_json_property_aggregation(
        &mut self,
        parent: &str,
        props: Option<&BTreeMap<String, JSONSchemaProps>>,
    ) -> Result<BTreeSet<String>> {
        props
            .map(|children_props| {
                children_props
                    .iter()
                    .map(|(name, prop)| self.parse_json_property(Some(parent), name, prop))
                    .collect::<Result<_>>()
            })
            .transpose()
            .map(Option::unwrap_or_default)
    }

    fn parse_field(&mut self, mut field: ModelFieldNativeSpec) -> Result<()> {
        // validate name
        let name = field.name.clone();
        assert_name(&name)?;

        // validate kind
        match &mut field.kind {
            // BEGIN primitive types
            ModelFieldKindNativeSpec::None {} => {}
            ModelFieldKindNativeSpec::Boolean { default: _ } => {}
            ModelFieldKindNativeSpec::Integer {
                default,
                minimum,
                maximum,
            } => {
                assert_cmp(&name, "default", default, "maximum", maximum)?;
                assert_cmp(&name, "minimum", minimum, "default", default)?;
                assert_cmp(&name, "minimum", minimum, "maximum", maximum)?;
            }
            ModelFieldKindNativeSpec::Number {
                default,
                minimum,
                maximum,
            } => {
                assert_cmp(&name, "default", default, "maximum", maximum)?;
                assert_cmp(&name, "minimum", minimum, "default", default)?;
                assert_cmp(&name, "minimum", minimum, "maximum", maximum)?;
            }
            ModelFieldKindNativeSpec::String { default, kind } => match kind {
                ModelFieldKindStringSpec::Dynamic {} => {}
                ModelFieldKindStringSpec::Static { length } => {
                    if let Some(default) = default.as_ref().map(|default| default.len()) {
                        let default = default.try_into()?;
                        assert_cmp(&name, "default", &Some(default), "length", &Some(*length))?;
                        assert_cmp(&name, "length", &Some(*length), "default", &Some(default))?;
                    }
                }
                ModelFieldKindStringSpec::Range { minimum, maximum } => {
                    if let Some(default) = default.as_ref().map(|default| default.len()) {
                        let default = default.try_into()?;
                        assert_cmp(&name, "default", &Some(default), "maximum", &Some(*maximum))?;
                        assert_cmp(&name, "minimum", minimum, "default", &Some(default))?;
                    }
                    assert_cmp(&name, "minimum", minimum, "maximum", &Some(*maximum))?;
                }
            },
            ModelFieldKindNativeSpec::OneOfStrings { default, choices } => {
                assert_contains(&name, "choices", choices, "default", default.as_ref())?;
            }
            // BEGIN string formats
            ModelFieldKindNativeSpec::DateTime { default: _ } => {}
            ModelFieldKindNativeSpec::Ip {} => {}
            ModelFieldKindNativeSpec::Uuid {} => {}
            // BEGIN aggregation types
            ModelFieldKindNativeSpec::Object {
                children,
                dynamic: _,
            }
            | ModelFieldKindNativeSpec::ObjectArray { children } => {
                *children = children
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
            }
        }

        self.insert_field(name.clone(), &name, field)
    }

    fn merge_fields(&mut self, parent: &str, fields: ModelFieldsNativeSpec) -> Result<()> {
        for field in fields {
            self.merge_field(parent, field)?;
        }
        Ok(())
    }

    fn merge_field(&mut self, parent: &str, mut field: ModelFieldNativeSpec) -> Result<()> {
        // merge name
        field.name = merge_name(parent, &field.name)?;

        // merge kind
        if let Some(children) = field.kind.get_children_mut() {
            *children = children
                .iter()
                .map(|child| merge_name(parent, child))
                .collect::<Result<_>>()?;
        }

        self.parse_field(field)
    }

    fn finalize(mut self) -> Result<ModelFieldsNativeSpec> {
        fn finalize_aggregation_type<'k, 'c, Keys, Key, Children, Child>(
            map: &mut ModelFieldsNativeMap,
            keys: &'k Keys,
            name: &str,
            children: &'c Children,
        ) -> Result<()>
        where
            &'k Keys: IntoIterator<Item = &'k Key>,
            Key: 'k + fmt::Debug + PartialEq<Child>,
            &'c Children: IntoIterator<Item = &'c Child>,
            Child: 'c + fmt::Debug + AsRef<str>,
        {
            for child in children {
                assert_child(name, "children", child)?;
                assert_contains(name, "fields", keys, "children", Some(child))?;
            }

            let parent = parent_name(name);
            create_parent_object(map, parent)
        }

        fn create_parent_object(map: &mut ModelFieldsNativeMap, name: Option<&str>) -> Result<()> {
            match name {
                Some(name) => {
                    let children: BTreeSet<_> = map
                        .keys()
                        .filter(|child_name| parent_name(child_name) == Some(name))
                        .cloned()
                        .collect();

                    match map.get(name) {
                        Some(field) => match field.kind.get_children() {
                            Some(given) => assert_children(name, &children, given),
                            None => Ok(()),
                        },
                        None => {
                            let field = ModelFieldNativeSpec {
                                name: name.to_string(),
                                kind: ModelFieldKindNativeSpec::Object {
                                    children: children.into_iter().collect(),
                                    dynamic: false,
                                },
                                attribute: ModelFieldAttributeSpec { optional: false },
                            };
                            map.insert(name.to_string(), field);

                            create_parent_object(map, parent_name(name))
                        }
                    }
                }
                None => Ok(()),
            }
        }

        fn assert_children<'e, 'g, ExpectedList, Expected, GivenList, Given>(
            name: &str,
            expected: &'e ExpectedList,
            given: &'g GivenList,
        ) -> Result<()>
        where
            &'e ExpectedList: IntoIterator<Item = &'e Expected>,
            Expected: 'e + fmt::Display + PartialEq<Given>,
            &'g GivenList: IntoIterator<Item = &'g Given>,
            Given: 'g + fmt::Display + PartialEq<Expected>,
        {
            assert_children_by(name, expected, given)?;
            assert_children_by(name, given, expected)?;
            Ok(())
        }

        fn assert_children_by<'e, 'g, ExpectedList, Expected, GivenList, Given>(
            name: &str,
            expected: &'e ExpectedList,
            given: &'g GivenList,
        ) -> Result<()>
        where
            &'e ExpectedList: IntoIterator<Item = &'e Expected>,
            Expected: 'e + fmt::Display + PartialEq<Given>,
            &'g GivenList: IntoIterator<Item = &'g Given>,
            Given: 'g,
        {
            let missed: Vec<_> = expected
                .into_iter()
                .filter(|&expected| given.into_iter().all(|given| expected != given))
                .collect();

            if missed.is_empty() {
                Ok(())
            } else {
                let missed = missed.iter().join(", ");
                bail!("cannot find the children fields of {name:?}: {missed:?}")
            }
        }

        let keys: Vec<_> = self.map.keys().cloned().collect();

        // parse aggregation types
        for (name, field) in self.map.clone() {
            if let Some(children) = field.kind.get_children() {
                finalize_aggregation_type(&mut self.map, &keys, &name, children)?;
            }
        }

        Ok(self.map.into_values().collect())
    }
}

fn merge_name(parent: &str, name: &str) -> Result<String> {
    assert_name(parent)?;
    assert_name(name)?;

    let parent = &parent[..parent.len() - 1];
    Ok(format!("{parent}{name}"))
}

fn parent_name(name: &str) -> Option<&str> {
    name.rmatch_indices('/')
        .map(|(index, _)| &name[..=index])
        .nth(1)
}

fn convert_name(parent: Option<&str>, name: &str) -> Result<String> {
    let converted = name.to_snake_case();

    if parent.is_some() || !name.is_empty() {
        let re = Regex::new(name::RE_CHILD)?;
        if !re.is_match(&converted) {
            bail!("property name is invalid: {name} {converted:?}");
        }
    }

    match parent {
        Some(parent) => Ok(format!("{parent}{converted}/")),
        None => Ok(format!("/{converted}")),
    }
}

fn assert_name(name: &str) -> Result<()> {
    let re = Regex::new(name::RE)?;
    if re.is_match(name) {
        Ok(())
    } else {
        bail!("field name is invalid: {name} {name:?}")
    }
}

fn assert_child<T>(parent: &str, child_label: &str, child: &T) -> Result<()>
where
    T: AsRef<str>,
{
    let child = child.as_ref();

    if child.starts_with(parent) {
        Ok(())
    } else {
        bail!("{child_label} value {child:?} is not a child of {parent:?}")
    }
}

fn assert_cmp<T>(
    name: &str,
    a_label: &str,
    a: &Option<T>,
    b_label: &str,
    b: &Option<T>,
) -> Result<()>
where
    T: Copy + fmt::Debug + PartialOrd,
{
    match (a, b) {
        (Some(a), Some(b)) => {
            if a <= b {
                Ok(())
            } else {
                bail!("{a_label} value {a:?} should be less than {b_label} value {b:?}: {name:?}")
            }
        }
        _ => Ok(()),
    }
}
