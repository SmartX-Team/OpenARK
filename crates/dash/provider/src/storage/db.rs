use std::{borrow::Borrow, collections::BTreeMap, fmt};

use anyhow::{anyhow, bail, Result};
use chrono::{NaiveDateTime, Utc};
use dash_api::{
    model::{
        ModelCrd, ModelFieldDateTimeDefaultType, ModelFieldKindNativeSpec,
        ModelFieldKindObjectSpec, ModelFieldKindStringSpec, ModelFieldNativeSpec,
        ModelFieldsNativeSpec, ModelState,
    },
    storage::db::{
        ModelStorageDatabaseBorrowedSpec, ModelStorageDatabaseOwnedSpec, ModelStorageDatabaseSpec,
    },
};
use kube::ResourceExt;
use sea_orm::{
    sea_query::{ColumnDef, Expr, IntoIden, Table, TableRef},
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, ConnectionTrait, Database,
    DatabaseConnection, DbErr, DeriveEntityModel, DerivePrimaryKey, DeriveRelation, EntityTrait,
    EnumIter, Iden, PrimaryKeyTrait, QueryFilter, QueryOrder, QueryResult, Schema, Statement,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use tracing::{instrument, Level};

pub struct DatabaseStorageClient {
    db: DatabaseConnection,
}

impl<'model> DatabaseStorageClient {
    const NATIVE_URL: &'static str = "postgres://dash-postgres/dash";

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    pub async fn try_new(storage: &ModelStorageDatabaseSpec) -> Result<Self> {
        Ok(Self {
            db: Self::load_storage(storage).await?,
        })
    }

    #[instrument(level = Level::INFO, err(Display))]
    async fn load_storage(storage: &ModelStorageDatabaseSpec) -> Result<DatabaseConnection> {
        let db = match storage {
            ModelStorageDatabaseSpec::Borrowed(storage) => {
                Self::load_storage_by_borrowed(storage).await?
            }
            ModelStorageDatabaseSpec::Owned(storage) => {
                Self::load_storage_by_owned(storage).await?
            }
        };

        Entity::init(&db).await.map(|()| db).map_err(Into::into)
    }

    #[instrument(level = Level::INFO, err(Display))]
    async fn load_storage_by_borrowed(
        storage: &ModelStorageDatabaseBorrowedSpec,
    ) -> Result<DatabaseConnection> {
        Database::connect(storage.url.as_str())
            .await
            .map_err(Into::into)
    }

    #[instrument(level = Level::INFO, err(Display))]
    async fn load_storage_by_owned(
        storage: &ModelStorageDatabaseOwnedSpec,
    ) -> Result<DatabaseConnection> {
        let ModelStorageDatabaseOwnedSpec {} = storage;
        let storage = ModelStorageDatabaseBorrowedSpec {
            url: Self::NATIVE_URL.parse()?,
        };

        Self::load_storage_by_borrowed(&storage).await
    }
}

impl DatabaseStorageClient {
    pub fn get_session<'model>(&self, model: &'model ModelCrd) -> DatabaseStorageSession<'model> {
        DatabaseStorageSession {
            db: self.db.clone(),
            model,
        }
    }
}

pub struct DatabaseStorageSession<'model> {
    db: DatabaseConnection,
    model: &'model ModelCrd,
}

impl<'model> DatabaseStorageSession<'model> {
    fn get_table_name(&self) -> (String, RuntimeIden) {
        let name = self.model.name_any();
        let iden = RuntimeIden::from_str(&name);
        (name, iden)
    }

    fn get_model_name_column(&self) -> Result<String> {
        // TODO: to be implemented (maybe in ModelCRD?)
        let (name, _) = self.get_table_name();
        bail!("cannot infer name column: {name:?}")
    }

    fn get_model_hash(&self) -> Result<RuntimeIden> {
        self.get_model_fields_to_json_vec()
            .map(RuntimeIden::from_bytes)
    }

    fn get_model_version(&self) -> Option<i64> {
        self.model.metadata.generation
    }

    fn get_model_fields(&self) -> Result<&ModelFieldsNativeSpec> {
        match &self.model.status {
            Some(status) if status.state == ModelState::Ready => match &status.fields {
                Some(fields) => Ok(fields),
                None => {
                    let name = self.model.name_any();
                    bail!("model has no fields status: {name:?}")
                }
            },
            Some(_) | None => {
                let name = self.model.name_any();
                bail!("model is not ready: {name:?}")
            }
        }
    }

    fn get_model_fields_to_json_value(&self) -> Result<Value> {
        self.get_model_fields()
            .and_then(|fields| ::serde_json::to_value(fields).map_err(Into::into))
    }

    fn get_model_fields_to_json_vec(&self) -> Result<Vec<u8>> {
        self.get_model_fields()
            .and_then(|fields| ::serde_json::to_vec(fields).map_err(Into::into))
    }

    fn get_model_columns(&self) -> Result<Columns> {
        self.get_model_fields().and_then(convert_fields_to_columns)
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn is_table_exists(&self) -> Result<bool> {
        let (_, table_name) = self.get_table_name();
        let statement = Statement::from_string(
            self.db.get_database_backend(),
            format!(r#"SELECT "{table_name}"."_id" FROM "{table_name}" LIMIT 0"#),
        );

        Ok(self.db.execute(statement).await.is_ok())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get(&self, ref_name: &str) -> Result<Option<Value>> {
        let (_, table_name) = self.get_table_name();
        let column_name = self.get_model_name_column()?;
        let statement = Statement::from_string(
            self.db.get_database_backend(),
            format!(
                r#"SELECT * FROM "{table_name}" WHERE "{table_name}"."{column_name}" = {ref_name} LIMIT 1"#
            ),
        );

        let row = self.db.query_one(statement).await?;
        let fields = self.get_model_fields()?;

        match row {
            Some(row) => parse_query_result(&row, fields).map(Some),
            None => Ok(None),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn get_list(&self) -> Result<Vec<Value>> {
        const LIMIT: usize = 30;

        let (_, table_name) = self.get_table_name();
        let statement = Statement::from_string(
            self.db.get_database_backend(),
            format!(r#"SELECT * FROM "{table_name}" LIMIT {LIMIT}"#),
        );

        let rows = self.db.query_all(statement).await?;
        let fields = self.get_model_fields()?;

        rows.into_iter()
            .map(|row| parse_query_result(&row, fields))
            .collect()
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_current_table_fields(&self) -> Result<Option<ModelFieldsNativeSpec>> {
        let (name, table_name) = self.get_table_name();
        let model_hash = self.get_model_hash()?;

        match self.get_model_version() {
            Some(model_version) if model_version == i64::MAX => bail!("validation error: table version overflow ({name:?}): maybe we need to increase the capacity"),
            Some(model_version) => {
                match Entity::find()
                    .order_by_desc(Column::Id)
                    .filter(Column::ModelName.like(table_name.as_ref()))
                    .one(&self.db)
                    .await
                {
                    Ok(Some(table)) => if model_version == table.model_version {
                        if model_hash.as_ref() == table.model_hash.as_str() {
                            ::serde_json::from_value(table.model_value).map_err(Into::into)
                        } else {
                            let table_hash = table.model_hash;
                            bail!("validation error: model nonce mismatch ({name:?}): expected {model_hash:?}, but given {table_hash:?}")
                        }
                    } else {
                        let table_version = table.model_version;
                        bail!("validation error: model version mismatch ({name:?}): expected {model_version:?}, but given {table_version:?}")
                    }
                    Ok(None) => Ok(None),
                    Err(e) => {
                        bail!("validation error: failed to load the model metadata ({name:?}): {e}")
                    }
                }
            }
            #[cfg(feature = "i-want-to-cleanup-all-before-running-for-my-testing")]
            None => {
                self.delete_table().await?;
                Ok(None)
            }
            #[cfg(not(feature = "i-want-to-cleanup-all-before-running-for-my-testing"))]
            None => bail!("validation error: the table {name:?} already exists"),
        }
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn get_current_table_columns(&self) -> Result<Columns> {
        self.get_current_table_fields()
            .await
            .and_then(|fields| {
                fields.ok_or_else(|| {
                    let name = self.model.name_any();
                    anyhow!("failed to find the table metadata: {name:?}")
                })
            })
            .and_then(|fields| convert_fields_to_columns(&fields))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn create_table(&self) -> Result<()> {
        if self.is_table_exists().await? {
            return Ok(());
        }

        let (name, table_name) = self.create_table_metadata().await?;

        let mut statement = Table::create();
        let statement = statement.table(TableRef::Table(table_name.into_iden()));

        // metadata
        statement
            .col(
                ColumnDef::new(RuntimeIden("_id"))
                    .auto_increment()
                    .primary_key()
                    .integer(),
            )
            .col(
                ColumnDef::new(RuntimeIden("_metadata__created_at"))
                    .timestamp()
                    .default(Expr::current_timestamp()),
            )
            .col(
                ColumnDef::new(RuntimeIden("_metadata__updated_at"))
                    .timestamp()
                    .default(Expr::current_timestamp()),
            );

        // collect fields
        for (_, mut column) in self.get_model_columns()? {
            statement.col(&mut column);
        }

        let builder = self.db.get_database_backend();
        let statement = builder.build(statement);
        if let Err(e) = self.db.execute(statement).await {
            bail!("migration error: failed to create table {name:?}: {e}");
        }
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    async fn create_table_metadata(&self) -> Result<(String, RuntimeIden)> {
        let (name, table_name) = self.get_table_name();

        let model = ActiveModel {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::Set(Utc::now().naive_utc()),
            model_hash: ActiveValue::Set(self.get_model_hash()?.0),
            model_name: ActiveValue::Set(table_name.0.clone()),
            model_value: ActiveValue::Set(self.get_model_fields_to_json_value()?),
            model_version: ActiveValue::Set(self.get_model_version().unwrap_or_default()),
        };

        model.insert(&self.db).await?;
        Ok((name, table_name))
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn update_table(&self) -> Result<()> {
        if !self.is_table_exists().await? {
            return self.create_table().await;
        }

        let (name, table_name) = self.get_table_name();

        let mut statement = Table::alter();
        let statement = statement.table(TableRef::Table(table_name.into_iden()));

        // collect fields
        let fields_from_last = self.get_current_table_columns().await?;
        let fields_from_now = self.get_model_columns()?;

        // fields: drop
        for field_name in fields_from_last.keys() {
            if !fields_from_now.contains_key(field_name) {
                statement.drop_column(field_name.clone());
            }
        }

        // fields: create, update
        for (field_name, mut field_now) in fields_from_now {
            // FIXME: detect RENAME columns
            // FIXME: protect from mistake
            match fields_from_last.get(&field_name) {
                // fields: create
                // FIXME: define default values (NULLABLE or DEFAULT)
                None => statement.add_column_if_not_exists(&mut field_now),

                // fields: update
                Some(_) => statement.modify_column(&mut field_now),
            };
        }

        let builder = self.db.get_database_backend();
        let statement = builder.build(statement);
        if let Err(e) = self.db.execute(statement).await {
            bail!("migration error: failed to update table {name:?}: {e}");
        }
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), err(Display))]
    pub async fn delete_table(&self) -> Result<()> {
        let (_, table_name) = self.get_table_name();

        let statement = Statement::from_string(
            self.db.get_database_backend(),
            format!(r#"DROP TABLE IF EXISTS "{table_name}" CASCADE"#),
        );

        self.db.execute(statement).await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "__dash_model_migrations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(column_type = "Timestamp")]
    pub created_at: NaiveDateTime,
    #[sea_orm(column_type = "String(Some(64))")]
    pub model_hash: String,
    #[sea_orm(column_type = "String(Some(64))")]
    pub model_name: String,
    pub model_value: Value,
    pub model_version: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Entity {
    #[instrument(level = Level::INFO, skip(db), err(Display))]
    pub async fn init(db: &DatabaseConnection) -> Result<(), DbErr> {
        // Drop the old model table
        #[cfg(feature = "i-want-to-cleanup-all-before-running-for-my-testing")]
        {
            let builder = db.get_database_backend();
            let statement =
                builder.build(::sea_orm::sea_query::Table::drop().table(Self).if_exists());

            db.execute(statement).await?;
        }

        // Check whether the model table is already exist
        if Self::find_by_id(0).one(db).await.is_ok() {
            return Ok(());
        }

        // Create a model table
        {
            let builder = db.get_database_backend();
            let statement = builder
                .build(&Schema::new(db.get_database_backend()).create_table_from_entity(Self));

            db.execute(statement).await?;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeIden<T = String>(T);

impl<T> fmt::Debug for RuntimeIden<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as fmt::Debug>::fmt(&self.0, f)
    }
}

impl<T> fmt::Display for RuntimeIden<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <T as fmt::Display>::fmt(&self.0, f)
    }
}

impl<T> AsRef<str> for RuntimeIden<T>
where
    T: AsRef<str>,
{
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<T> Borrow<str> for RuntimeIden<T>
where
    T: Borrow<str>,
{
    fn borrow(&self) -> &str {
        self.0.borrow()
    }
}

impl RuntimeIden {
    fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        // create a Sha256 object
        let mut hasher = Sha256::new();

        // write input message
        hasher.update(bytes.as_ref());

        // read hash digest and consume hasher
        let hash = hasher.finalize();

        // encode to hex format
        Self(format!("{hash:x}"))
    }

    fn from_str(s: impl AsRef<str>) -> Self {
        // create a Sha256 object
        let mut hasher = Sha256::new();

        // write input message
        hasher.update(s.as_ref());

        // read hash digest and consume hasher
        let hash = hasher.finalize();

        // encode to hex format
        Self(format!("{hash:x}"))
    }
}

impl<T> Iden for RuntimeIden<T>
where
    T: Send + Sync + AsRef<str>,
{
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "{}", self.0.as_ref()).unwrap();
    }
}

type Columns = BTreeMap<RuntimeIden, ColumnDef>;

fn convert_fields_to_columns(
    fields: &ModelFieldsNativeSpec,
) -> Result<BTreeMap<RuntimeIden, ColumnDef>> {
    fields
        .iter()
        .filter_map(|field| {
            let name = RuntimeIden::from_str(&field.name);
            convert_field_to_column(&name, field)
                .map(|field| field.map(|field| (name, field)))
                .transpose()
        })
        .collect()
}

fn convert_field_to_column(
    name: &RuntimeIden,
    field: &ModelFieldNativeSpec,
) -> Result<Option<ColumnDef>> {
    let mut column = ColumnDef::new(name.clone());

    // attribute: optional
    if field.attribute.optional {
        column.null();
    } else {
        column.not_null();
    }

    match &field.kind {
        // BEGIN primitive types
        ModelFieldKindNativeSpec::None {} => Ok(None),
        ModelFieldKindNativeSpec::Boolean { default } => {
            // attribute: default
            if let Some(default) = default {
                column.default(*default);
            }

            // attribute: type
            column.boolean();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::Integer {
            default,
            minimum: _,
            maximum: _,
        } => {
            // attribute: default
            if let Some(default) = default {
                column.default(*default);
            }

            // attribute: type
            column.integer();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::Number {
            default,
            minimum: _,
            maximum: _,
        } => {
            // attribute: default
            if let Some(default) = default {
                column.default(**default);
            }

            // attribute: type
            column.double();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::String { default, kind } => {
            // attribute: default
            if let Some(default) = default {
                column.default(default.clone());
            }

            // attribute: length, type
            match kind {
                ModelFieldKindStringSpec::Dynamic {} => column.text(),
                ModelFieldKindStringSpec::Static { length } => column.char_len(*length),
                ModelFieldKindStringSpec::Range {
                    minimum: _,
                    maximum,
                } => column.string_len(*maximum),
            };
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::OneOfStrings { default, choices } => {
            // attribute: default
            if let Some(default) = default {
                column.default(default.clone());
            }

            // attribute: type, choices
            column.enumeration(name.clone(), choices.iter().map(RuntimeIden::from_str));
            Ok(Some(column))
        }
        // BEGIN string formats
        ModelFieldKindNativeSpec::DateTime { default } => {
            // attribute: default
            if let Some(default) = default {
                match default {
                    ModelFieldDateTimeDefaultType::Now => {
                        column.default("CURRENT_TIMESTAMP");
                    }
                }
            }

            // attribute: type
            column.timestamp();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::Ip {} => {
            // attribute: type
            column.inet();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::Uuid {} => {
            // attribute: type
            column.uuid();
            Ok(Some(column))
        }
        // BEGIN aggregation types
        ModelFieldKindNativeSpec::StringArray {} => {
            // attribute: type
            column.json();
            Ok(Some(column))
        }
        ModelFieldKindNativeSpec::Object { children: _, kind } => {
            // attribute: type
            match kind {
                ModelFieldKindObjectSpec::Dynamic {} => {
                    column.json();
                    Ok(Some(column))
                }
                ModelFieldKindObjectSpec::Enumerate { choices } => {
                    // attribute: default
                    if let Some(default) = choices.first() {
                        column.default(default);
                    }

                    // attribute: choices
                    column.enumeration(name.clone(), choices.iter().map(RuntimeIden::from_str));
                    Ok(Some(column))
                }
                ModelFieldKindObjectSpec::Static {} => Ok(None),
            }
        }
        ModelFieldKindNativeSpec::ObjectArray { children: _ } => {
            // attribute: type
            column.json();
            Ok(Some(column))
        }
    }
}

fn parse_query_result(row: &QueryResult, fields: &[ModelFieldNativeSpec]) -> Result<Value> {
    let mut value = Map::default();
    for field in fields {
        value.insert(field.name.clone(), parse_query_result_column(row, field)?);
    }
    Ok(Value::Object(value))
}

fn parse_query_result_column(row: &QueryResult, field: &ModelFieldNativeSpec) -> Result<Value> {
    match &field.kind {
        // BEGIN primitive types
        ModelFieldKindNativeSpec::None { .. } => Ok(Value::Null),
        ModelFieldKindNativeSpec::Boolean { .. } => row
            .try_get_by::<bool, _>(field.name.as_str())
            .map(Into::into)
            .map_err(Into::into),
        ModelFieldKindNativeSpec::Integer { .. } => row
            .try_get_by::<i64, _>(field.name.as_str())
            .map(Into::into)
            .map_err(Into::into),
        ModelFieldKindNativeSpec::Number { .. } => row
            .try_get_by::<f64, _>(field.name.as_str())
            .map(Into::into)
            .map_err(Into::into),
        ModelFieldKindNativeSpec::String { .. } | ModelFieldKindNativeSpec::OneOfStrings { .. } => {
            row.try_get_by::<String, _>(field.name.as_str())
                .map(Into::into)
                .map_err(Into::into)
        }
        // BEGIN string formats
        ModelFieldKindNativeSpec::DateTime { .. }
        | ModelFieldKindNativeSpec::Ip { .. }
        | ModelFieldKindNativeSpec::Uuid { .. } => row
            .try_get_by::<String, _>(field.name.as_str())
            .map(Into::into)
            .map_err(Into::into),
        // BEGIN string formats
        ModelFieldKindNativeSpec::StringArray { .. } => row
            .try_get_by::<Value, _>(field.name.as_str())
            .map(Into::into)
            .map_err(Into::into),
        ModelFieldKindNativeSpec::Object { .. } | ModelFieldKindNativeSpec::ObjectArray { .. } => {
            row.try_get_by::<Value, _>(field.name.as_str())
                .map_err(Into::into)
        }
    }
}
