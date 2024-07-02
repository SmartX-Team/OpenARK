use anyhow::{Error, Result};
use chrono::NaiveDateTime;
use kubegraph_api::market::{product::ProductSpec, r#pub::PubSpec, sub::SubSpec, BaseModel};
use sea_orm::{
    ActiveModelBehavior, ActiveValue, DeriveActiveEnum, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};
use serde_json::Value;

type Id = <ProductSpec as BaseModel>::Id;
type Cost = <ProductSpec as BaseModel>::Cost;
type Count = <ProductSpec as BaseModel>::Count;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "prices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    pub product_id: Id,
    #[sea_orm(column_type = "Timestamp")]
    pub created_at: NaiveDateTime,
    pub direction: Direction,
    pub cost: Cost,
    pub count: Count,
    pub spec: Value,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum Direction {
    Pub = 0,
    Sub = 1,
}

impl From<Direction> for ::kubegraph_api::market::price::Direction {
    fn from(value: Direction) -> Self {
        match value {
            Direction::Pub => Self::Pub,
            Direction::Sub => Self::Sub,
        }
    }
}

impl From<::kubegraph_api::market::price::Direction> for Direction {
    fn from(value: ::kubegraph_api::market::price::Direction) -> Self {
        match value {
            ::kubegraph_api::market::price::Direction::Pub => Self::Pub,
            ::kubegraph_api::market::price::Direction::Sub => Self::Sub,
        }
    }
}

impl TryFrom<Model> for PubSpec {
    type Error = Error;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let Model {
            id: _,
            product_id: _,
            created_at: _,
            direction: _,
            cost,
            count,
            spec,
        } = value;

        let function = ::serde_json::from_value(spec)?;

        Ok(Self {
            cost,
            count,
            function,
        })
    }
}

impl TryFrom<Model> for SubSpec {
    type Error = Error;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let Model {
            id: _,
            product_id: _,
            created_at: _,
            direction: _,
            cost,
            count,
            spec,
        } = value;

        let function = ::serde_json::from_value(spec)?;

        Ok(Self {
            cost,
            count,
            function,
        })
    }
}

impl ActiveModel {
    pub const fn from_id(id: Id) -> Self {
        Self {
            id: ActiveValue::Set(id),
            product_id: ActiveValue::NotSet,
            created_at: ActiveValue::NotSet,
            direction: ActiveValue::NotSet,
            cost: ActiveValue::NotSet,
            count: ActiveValue::NotSet,
            spec: ActiveValue::NotSet,
        }
    }

    pub fn from_pub_spec(spec: PubSpec, prod_id: Option<Id>, pub_id: Id) -> Result<Self> {
        let PubSpec {
            count,
            cost,
            function,
        } = spec;

        let spec = ::serde_json::to_value(function)?;

        Ok(Self {
            id: ActiveValue::Set(pub_id),
            product_id: match prod_id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            created_at: ActiveValue::NotSet,
            direction: ActiveValue::Set(Direction::Pub),
            cost: ActiveValue::Set(cost),
            count: ActiveValue::Set(count),
            spec: ActiveValue::Set(spec),
        })
    }

    pub fn from_sub_spec(spec: SubSpec, prod_id: Option<Id>, sub_id: Id) -> Result<Self> {
        let SubSpec {
            cost,
            count,
            function,
        } = spec;

        let spec = ::serde_json::to_value(function)?;

        Ok(Self {
            id: ActiveValue::Set(sub_id),
            product_id: match prod_id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            created_at: ActiveValue::NotSet,
            direction: ActiveValue::Set(Direction::Sub),
            cost: ActiveValue::Set(cost),
            count: ActiveValue::Set(count),
            spec: ActiveValue::Set(spec),
        })
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::product::Entity",
        from = "self::Column::ProductId",
        to = "super::product::Column::Id"
    )]
    Products,
}

impl ActiveModelBehavior for ActiveModel {}
