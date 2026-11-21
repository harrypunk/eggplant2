use sea_orm::entity::prelude::*;

use super::agent_runs;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "agent_tool_calls")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub session_id: String,
    pub run_id: i64,
    pub sequence: i64,
    pub call_id: String,
    pub name: String,
    pub arguments_json: String,
    pub status: String,
    pub result_json: Option<String>,
    pub result_content: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "agent_runs::Entity",
        from = "Column::RunId",
        to = "agent_runs::Column::Id"
    )]
    AgentRuns,
}

impl Related<agent_runs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AgentRuns.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
