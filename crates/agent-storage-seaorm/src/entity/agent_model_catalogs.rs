use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_model_catalogs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub provider_id: String,
    /// JSON array of model IDs, without credentials or provider secrets.
    pub models_json: String,
    pub checked_at: i64,
    pub last_modified: Option<i64>,
    pub etag: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
