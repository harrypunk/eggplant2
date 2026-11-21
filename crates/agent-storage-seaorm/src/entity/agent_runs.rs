use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub session_id: String,
    pub run_number: i64,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub provider: String,
    pub model: Option<String>,
    pub editor_context: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
