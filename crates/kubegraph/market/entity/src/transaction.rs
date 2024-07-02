use chrono::NaiveDateTime;
use kubegraph_api::market::{
    product::ProductSpec,
    transaction::{TaskSpec, TransactionSpec, TransactionTemplate},
    BaseModel,
};
use sea_orm::{
    ActiveModelBehavior, ActiveValue, DeriveActiveEnum, DeriveEntityModel, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait,
};

type Id = <ProductSpec as BaseModel>::Id;
type Cost = <ProductSpec as BaseModel>::Cost;
type Count = <ProductSpec as BaseModel>::Count;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: Id,
    pub pub_id: Id,
    pub sub_id: Id,
    #[sea_orm(column_type = "Timestamp")]
    pub created_at: NaiveDateTime,
    pub cost: Cost,
    pub count: Count,
    #[sea_orm(column_type = "Timestamp")]
    pub pub_state: TaskState,
    #[sea_orm(column_type = "Timestamp")]
    pub pub_updated_at: NaiveDateTime,
    #[sea_orm(column_type = "Timestamp")]
    pub sub_state: TaskState,
    #[sea_orm(column_type = "Timestamp")]
    pub sub_updated_at: NaiveDateTime,
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter, DeriveActiveEnum,
)]
#[sea_orm(rs_type = "i16", db_type = "SmallInteger")]
#[repr(i16)]
pub enum TaskState {
    #[default]
    Running = 0,
    Completed = 1,
    Failed = 2,
}

impl From<TaskState> for ::kubegraph_api::market::transaction::TaskState {
    fn from(value: TaskState) -> Self {
        match value {
            TaskState::Running => Self::Running,
            TaskState::Completed => Self::Completed,
            TaskState::Failed => Self::Failed,
        }
    }
}

impl From<::kubegraph_api::market::transaction::TaskState> for TaskState {
    fn from(value: ::kubegraph_api::market::transaction::TaskState) -> Self {
        match value {
            ::kubegraph_api::market::transaction::TaskState::Running => Self::Running,
            ::kubegraph_api::market::transaction::TaskState::Completed => Self::Completed,
            ::kubegraph_api::market::transaction::TaskState::Failed => Self::Failed,
        }
    }
}

impl From<Model> for TransactionSpec {
    fn from(value: Model) -> Self {
        let Model {
            id: _,
            pub_id: r#pub,
            sub_id: sub,
            created_at,
            cost,
            count,
            pub_state,
            pub_updated_at,
            sub_state,
            sub_updated_at,
        } = value;

        Self {
            template: TransactionTemplate {
                r#pub,
                sub,
                cost,
                count,
            },
            timestamp: created_at.and_utc(),
            pub_spec: TaskSpec {
                timestamp: pub_updated_at.and_utc(),
                state: pub_state.into(),
            },
            sub_spec: TaskSpec {
                timestamp: sub_updated_at.and_utc(),
                state: sub_state.into(),
            },
        }
    }
}

impl ActiveModel {
    pub const fn from_id(id: Id) -> Self {
        Self {
            id: ActiveValue::Set(id),
            pub_id: ActiveValue::NotSet,
            sub_id: ActiveValue::NotSet,
            created_at: ActiveValue::NotSet,
            cost: ActiveValue::NotSet,
            count: ActiveValue::NotSet,
            pub_state: ActiveValue::NotSet,
            pub_updated_at: ActiveValue::NotSet,
            sub_state: ActiveValue::NotSet,
            sub_updated_at: ActiveValue::NotSet,
        }
    }

    pub const fn from_template(id: Id, template: TransactionTemplate) -> Self {
        let TransactionTemplate {
            r#pub: pub_id,
            sub: sub_id,
            cost,
            count,
        } = template;

        Self {
            id: ActiveValue::Set(id),
            pub_id: ActiveValue::Set(pub_id),
            sub_id: ActiveValue::Set(sub_id),
            created_at: ActiveValue::NotSet,
            cost: ActiveValue::Set(cost),
            count: ActiveValue::Set(count),
            pub_state: ActiveValue::Set(TaskState::Running),
            pub_updated_at: ActiveValue::NotSet,
            sub_state: ActiveValue::Set(TaskState::Running),
            sub_updated_at: ActiveValue::NotSet,
        }
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::price::Entity",
        from = "self::Column::PubId",
        to = "super::price::Column::Id"
    )]
    Pubs,
    #[sea_orm(
        belongs_to = "super::price::Entity",
        from = "self::Column::SubId",
        to = "super::price::Column::Id"
    )]
    Subs,
}

impl ActiveModelBehavior for ActiveModel {}
