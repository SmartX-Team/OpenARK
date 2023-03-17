use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use dash_api::model::{
    ModelCrd, ModelCustomResourceDefinitionRefSpec, ModelFieldKindSpec, ModelFieldSpec,
    ModelFieldsSpec, ModelSpec, ModelState,
};
use inflector::Inflector;
use ipis::{
    core::anyhow::{bail, Result},
    itertools::Itertools,
};
use kiss_api::{
    k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::{
        CustomResourceDefinition, CustomResourceDefinitionVersion, JSONSchemaProps,
    },
    kube::{Api, Client},
};
use regex::Regex;

pub struct ModelValidator<'a> {
    pub kube: &'a Client,
}

impl<'a> ModelValidator<'a> {
    pub async fn validate_model(&self, spec: ModelSpec) -> Result<ModelFieldsSpec> {
        match spec {
            ModelSpec::Fields(spec) => self.validate_fields(spec).await,
            ModelSpec::CustomResourceDefinitionRef(spec) => {
                self.validate_custom_resource_definition_ref(spec).await
            }
        }
    }

    pub async fn validate_fields(&self, spec: ModelFieldsSpec) -> Result<ModelFieldsSpec> {
        let mut parser = ModelFieldsParser::default();
        for field in spec {
            match field.kind {
                // BEGIN primitive types
                ModelFieldKindSpec::Boolean { .. }
                | ModelFieldKindSpec::Integer { .. }
                | ModelFieldKindSpec::Number { .. }
                | ModelFieldKindSpec::String { .. }
                | ModelFieldKindSpec::OneOfStrings { .. }
                // BEGIN string formats
                | ModelFieldKindSpec::DateTime { .. }
                | ModelFieldKindSpec::Ip { .. }
                | ModelFieldKindSpec::Uuid { .. }
                // BEGIN aggregation types
                | ModelFieldKindSpec::Array { .. }
                | ModelFieldKindSpec::Object { .. }
                // END aggregation types
                    => parser.parse_field(field)?,
                // BEGIN reference types
                ModelFieldKindSpec::Model { name } => self
                    .validate_field_model(&name)
                    .await
                    .and_then(|fields| parser.merge_fields(&field.name, fields))?,
            }
        }

        parser.finalize()
    }

    async fn validate_field_model(&self, model_name: &str) -> Result<ModelFieldsSpec> {
        let api = Api::<ModelCrd>::all(self.kube.clone());
        let model = api.get(model_name).await?;
        let status = model.status;

        if !status
            .as_ref()
            .and_then(|status| status.state)
            .map(|state| state == ModelState::Ready)
            .unwrap_or_default()
        {
            bail!("model is not ready: {model_name:?}")
        }

        match status.and_then(|status| status.fields) {
            Some(fields) => Ok(fields),
            None => bail!("model has no fields status: {model_name:?}"),
        }
    }

    async fn validate_custom_resource_definition_ref(
        &self,
        spec: ModelCustomResourceDefinitionRefSpec,
    ) -> Result<ModelFieldsSpec> {
        let (crd_name, version) = {
            let mut attrs: Vec<_> = spec.name.split('/').collect();
            if attrs.len() != 2 {
                let crd_name = &spec.name;
                bail!(
                    "CRD name is invalid; expected name/version, but given {crd_name} {crd_name:?}",
                );
            }

            let version = attrs.pop().unwrap();
            let crd_name = attrs.pop().unwrap();
            (crd_name, version)
        };

        let api = Api::<CustomResourceDefinition>::all(self.kube.clone());
        let crd = api.get(crd_name).await?;

        match crd.spec.versions.iter().find(|def| def.name == version) {
            Some(def) => {
                let mut parser = ModelFieldsParser::default();
                parser.parse_custom_resource_definition(def)?;
                self.validate_fields(parser.finalize()?).await
            }
            None => bail!(
                "CRD version is invalid; expected one of {:?}, but given {version}",
                crd.spec.versions.iter().map(|def| &def.name).join(","),
            ),
        }
    }
}

type ModelFieldsMap = BTreeMap<String, ModelFieldSpec>;

#[derive(Debug, Default)]
struct ModelFieldsParser {
    map: ModelFieldsMap,
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

                Some(ModelFieldKindSpec::Boolean { default })
            }
            Some("integer") => {
                let default = prop.default.as_ref().and_then(|e| e.0.as_i64());
                let minimum = prop.minimum.as_ref().copied().map(|e| e.round() as i64);
                let maximum = prop.maximum.as_ref().copied().map(|e| e.round() as i64);

                Some(ModelFieldKindSpec::Integer {
                    default,
                    minimum,
                    maximum,
                })
            }
            Some("number") => {
                let default = prop.default.as_ref().and_then(|e| e.0.as_f64());
                let minimum = prop.minimum;
                let maximum = prop.maximum;

                Some(ModelFieldKindSpec::Number {
                    default,
                    minimum,
                    maximum,
                })
            }
            Some("string") => match prop.format.as_ref().map(AsRef::as_ref) {
                // BEGIN string format
                Some("date-time") => Some(ModelFieldKindSpec::DateTime { default: None }),
                Some("ip") => Some(ModelFieldKindSpec::Ip {}),
                Some("uuid") => Some(ModelFieldKindSpec::Uuid {}),
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

                        Some(ModelFieldKindSpec::OneOfStrings {
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

                        Some(ModelFieldKindSpec::String { default })
                    }
                },
                Some(format) => bail!("unknown string format of {name:?}: {format:?}"),
            },
            // BEGIN aggregation types
            Some("array") => {
                let children =
                    self.parse_json_property_aggregation(&name, prop.properties.as_ref())?;

                Some(ModelFieldKindSpec::Array {
                    children: children.into_iter().collect(),
                })
            }
            Some("object") => {
                let children =
                    self.parse_json_property_aggregation(&name, prop.properties.as_ref())?;

                Some(ModelFieldKindSpec::Object {
                    children: children.into_iter().collect(),
                    dynamic: false,
                })
            }
            type_ => bail!("unknown type of {name:?}: {type_:?}"),
        };

        match kind {
            Some(kind) => {
                let spec = ModelFieldSpec {
                    name: name.clone(),
                    kind,
                    nullable: prop.nullable.unwrap_or_default(),
                };

                self.insert_field(name.clone(), name_raw, spec)
                    .map(|()| name)
            }
            None => Ok(name),
        }
    }

    fn insert_field(&mut self, name: String, name_raw: &str, spec: ModelFieldSpec) -> Result<()> {
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

    fn parse_field(&mut self, mut field: ModelFieldSpec) -> Result<()> {
        // validate name
        let name = field.name.clone();
        assert_name(&name)?;

        // validate kind
        match &mut field.kind {
            // BEGIN primitive types
            ModelFieldKindSpec::Boolean { default: _ } => {}
            ModelFieldKindSpec::Integer {
                default,
                minimum,
                maximum,
            } => {
                assert_cmp(&name, "default", default, "maximum", maximum)?;
                assert_cmp(&name, "minimum", minimum, "default", default)?;
                assert_cmp(&name, "minimum", minimum, "maximum", maximum)?;
            }
            ModelFieldKindSpec::Number {
                default,
                minimum,
                maximum,
            } => {
                assert_cmp(&name, "default", default, "maximum", maximum)?;
                assert_cmp(&name, "minimum", minimum, "default", default)?;
                assert_cmp(&name, "minimum", minimum, "maximum", maximum)?;
            }
            ModelFieldKindSpec::String { default: _ } => {}
            ModelFieldKindSpec::OneOfStrings { default, choices } => {
                assert_contains(&name, "choices", choices, "default", default.as_ref())?;
            }
            // BEGIN string formats
            ModelFieldKindSpec::DateTime { default: _ } => {}
            ModelFieldKindSpec::Ip {} => {}
            ModelFieldKindSpec::Uuid {} => {}
            // BEGIN aggregation types
            ModelFieldKindSpec::Array { children } => {
                *children = children
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
            }
            ModelFieldKindSpec::Object {
                children,
                dynamic: _,
            } => {
                *children = children
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
            }
            // BEGIN reference types
            ModelFieldKindSpec::Model { .. } => {}
        }

        self.insert_field(name.clone(), &name, field)
    }

    fn merge_fields(&mut self, parent: &str, fields: ModelFieldsSpec) -> Result<()> {
        for field in fields {
            self.merge_field(parent, field)?;
        }
        Ok(())
    }

    fn merge_field(&mut self, parent: &str, mut field: ModelFieldSpec) -> Result<()> {
        // merge name
        field.name = merge_name(parent, &field.name)?;

        // merge kind
        match &mut field.kind {
            // BEGIN primitive types
            ModelFieldKindSpec::Boolean { .. }
            | ModelFieldKindSpec::Integer { .. }
            | ModelFieldKindSpec::Number { .. }
            | ModelFieldKindSpec::String { .. }
            | ModelFieldKindSpec::OneOfStrings { .. } => {}
            // BEGIN string formats
            ModelFieldKindSpec::DateTime { .. }
            | ModelFieldKindSpec::Ip {}
            | ModelFieldKindSpec::Uuid {} => {}
            // BEGIN aggregation types
            ModelFieldKindSpec::Array { children } => {
                *children = children
                    .iter()
                    .map(|child| merge_name(parent, child))
                    .collect::<Result<_>>()?;
            }
            ModelFieldKindSpec::Object {
                children,
                dynamic: _,
            } => {
                *children = children
                    .iter()
                    .map(|child| merge_name(parent, child))
                    .collect::<Result<_>>()?;
            }
            // BEGIN reference types
            ModelFieldKindSpec::Model { .. } => {}
        }

        self.parse_field(field)
    }

    fn finalize(mut self) -> Result<ModelFieldsSpec> {
        fn finalize_aggregation_type<'k, 'c, Keys, Key, Children, Child>(
            map: &mut ModelFieldsMap,
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

        fn create_parent_object(map: &mut ModelFieldsMap, name: Option<&str>) -> Result<()> {
            match name {
                Some(name) => {
                    let children: BTreeSet<_> = map
                        .keys()
                        .filter(|child_name| parent_name(child_name) == Some(name))
                        .cloned()
                        .collect();

                    match map.get(name) {
                        Some(field) => match &field.kind {
                            // BEGIN aggregation types
                            ModelFieldKindSpec::Array { children: given } => {
                                assert_children(name, &children, given)
                            }
                            ModelFieldKindSpec::Object {
                                children: given,
                                dynamic: _,
                            } => assert_children(name, &children, given),
                            // BEGIN primitive types
                            ModelFieldKindSpec::Boolean { .. }
                            | ModelFieldKindSpec::Integer { .. }
                            | ModelFieldKindSpec::Number { .. }
                            | ModelFieldKindSpec::String { .. }
                            | ModelFieldKindSpec::OneOfStrings { .. }
                            // BEGIN string formats
                            |  ModelFieldKindSpec::DateTime { .. }
                            | ModelFieldKindSpec::Ip {}
                            | ModelFieldKindSpec::Uuid {}
                            // BEGIN reference types
                            | ModelFieldKindSpec::Model { .. } => {
                                let type_ = field.kind.to_type();
                                bail!("parent field should be either Array or Object: {name:?} as {type_:?}")
                            }
                        },
                        None => {
                            let field = ModelFieldSpec {
                                name: name.to_string(),
                                kind: ModelFieldKindSpec::Object {
                                    children: children.into_iter().collect(),
                                    dynamic: false,
                                },
                                nullable: false,
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
            match &field.kind {
                // BEGIN primitive types
                ModelFieldKindSpec::Boolean { .. }
                | ModelFieldKindSpec::Integer { .. }
                | ModelFieldKindSpec::Number { .. }
                | ModelFieldKindSpec::String { .. }
                | ModelFieldKindSpec::OneOfStrings { .. } => {}
                // BEGIN string formats
                ModelFieldKindSpec::DateTime { .. }
                | ModelFieldKindSpec::Ip {}
                | ModelFieldKindSpec::Uuid {} => {}
                // BEGIN aggregation types
                ModelFieldKindSpec::Array { children } => {
                    finalize_aggregation_type(&mut self.map, &keys, &name, children)?
                }
                ModelFieldKindSpec::Object {
                    children,
                    dynamic: _,
                } => finalize_aggregation_type(&mut self.map, &keys, &name, children)?,
                // BEGIN reference types
                ModelFieldKindSpec::Model { .. } => {
                    let type_ = field.kind.to_type();
                    bail!("cannot finalize reference type: {name:?} as {type_:?}")
                }
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
        let re = Regex::new(NAME_CHILD_RE)?;
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
    let re = Regex::new(NAME_RE)?;
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

fn assert_contains<'a, List, ListItem, Item>(
    name: &str,
    list_label: &str,
    list: &'a List,
    item_label: &str,
    item: Option<&Item>,
) -> Result<()>
where
    &'a List: IntoIterator<Item = &'a ListItem>,
    ListItem: 'a + fmt::Debug + PartialEq<Item>,
    Item: fmt::Debug,
{
    match item {
        Some(item) => {
            if list.into_iter().any(|list_item| list_item == item) {
                Ok(())
            } else {
                let items = list
                    .into_iter()
                    .map(|list_item| format!("{list_item:?}"))
                    .join(", ");
                bail!(
                    "{item_label} value {item:?} should be one of {list_label} ({items}): {name:?}",
                )
            }
        }
        _ => Ok(()),
    }
}

const NAME_CHILD_RE: &str = r"^[a-z_-][a-z0-9_-]*[a-z0-9]?$";
const NAME_RE: &str = r"^/([a-z_-][a-z0-9_-]*[a-z0-9]?/)*$";
