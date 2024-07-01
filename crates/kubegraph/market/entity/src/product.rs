use anyhow::{Error, Result};
use chrono::NaiveDateTime;
use kubegraph_api::market::{product::ProductSpec, BaseModel};
use sea_orm::{
    ActiveModelBehavior, ActiveValue, DeriveEntityModel, DerivePrimaryKey, DeriveRelation,
    EntityTrait, EnumIter, PrimaryKeyTrait,
};
use serde_json::Value;

type Id = <ProductSpec as BaseModel>::Id;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "products")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    #[sea_orm(column_type = "Timestamp")]
    pub created_at: NaiveDateTime,
    pub spec: Value,
}

impl TryFrom<Model> for ProductSpec {
    type Error = Error;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let Model {
            id: _,
            created_at: _,
            spec,
        } = value;

        let problem = ::serde_json::from_value(spec)?;

        Ok(Self { problem })
    }
}

impl ActiveModel {
    pub const fn from_id(id: Id) -> Self {
        Self {
            id: ActiveValue::Set(id),
            created_at: ActiveValue::NotSet,
            spec: ActiveValue::NotSet,
        }
    }

    pub fn from_spec(spec: ProductSpec, id: Id) -> Result<Self> {
        let ProductSpec { problem } = spec;

        let spec = ::serde_json::to_value(problem)?;

        Ok(Self {
            id: ActiveValue::Set(id),
            created_at: ActiveValue::NotSet,
            spec: ActiveValue::Set(spec),
        })
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
