use futures_util::{TryStreamExt as _, future::BoxFuture, stream};
use sea_orm::{
    ActiveValue::Set,
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait, StatementBuilder,
    TransactionError, TransactionTrait,
    sea_query::{Alias, ColumnDef, Expr, ExprTrait, ForeignKey, ForeignKeyAction, Index, Table},
};

const INITIAL_MIGRATION: &str = "m20261121_000001_create_agent_storage";
const PROVIDER_CREDENTIALS_MIGRATION: &str = "m20261124_000002_create_agent_provider_credentials";
const PROVIDER_BASE_URL_MIGRATION: &str = "m20261124_000003_add_provider_base_url";

type Migration = for<'a> fn(&'a DatabaseTransaction) -> BoxFuture<'a, Result<(), DbErr>>;

const MIGRATIONS: &[(&str, Migration)] = &[
    (INITIAL_MIGRATION, initial_migration),
    (
        PROVIDER_CREDENTIALS_MIGRATION,
        provider_credentials_migration,
    ),
    (PROVIDER_BASE_URL_MIGRATION, provider_base_url_migration),
];

pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    execute(
        db,
        Table::create()
            .table("agent_schema_migrations")
            .if_not_exists()
            .col(text("name").primary_key())
            .to_owned(),
    )
    .await?;
    for (name, apply) in MIGRATIONS {
        if migration_history::Entity::find_by_id(*name)
            .one(db)
            .await?
            .is_some()
        {
            continue;
        }
        db.transaction(|transaction| {
            Box::pin(async move {
                apply(transaction).await?;
                migration_history::Entity::insert(migration_history::ActiveModel {
                    name: Set((*name).to_string()),
                })
                .exec(transaction)
                .await?;
                Ok(())
            })
        })
        .await
        .map_err(|error| match error {
            TransactionError::Connection(error) | TransactionError::Transaction(error) => error,
        })?;
    }
    Ok(())
}

fn initial_migration(transaction: &DatabaseTransaction) -> BoxFuture<'_, Result<(), DbErr>> {
    Box::pin(create_agent_storage(transaction))
}

fn provider_credentials_migration(
    transaction: &DatabaseTransaction,
) -> BoxFuture<'_, Result<(), DbErr>> {
    Box::pin(create_provider_credentials(transaction))
}

fn provider_base_url_migration(
    transaction: &DatabaseTransaction,
) -> BoxFuture<'_, Result<(), DbErr>> {
    Box::pin(add_provider_base_url(transaction))
}

async fn create_agent_storage<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    execute(
        db,
        Table::create()
            .table("agent_sessions")
            .if_not_exists()
            .col(text("id").primary_key())
            .col(big_integer("created_at"))
            .col(big_integer("updated_at"))
            .col(text("workspace").null())
            .col(text("title").null())
            .to_owned(),
    )
    .await?;
    execute(
        db,
        Table::create()
            .table("agent_runs")
            .if_not_exists()
            .col(big_integer("id").primary_key().auto_increment())
            .col(text("session_id"))
            .col(big_integer("run_number"))
            .col(text("status"))
            .col(text("error").null())
            .col(big_integer("created_at"))
            .col(big_integer("completed_at").null())
            .col(text("provider"))
            .col(text("model").null())
            .col(text("editor_context").null())
            .foreign_key(
                ForeignKey::create()
                    .from("agent_runs", "session_id")
                    .to("agent_sessions", "id")
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned(),
    )
    .await?;
    execute(
        db,
        Table::create()
            .table("agent_messages")
            .if_not_exists()
            .col(big_integer("id").primary_key().auto_increment())
            .col(text("session_id"))
            .col(big_integer("run_id"))
            .col(big_integer("sequence"))
            .col(text("role"))
            .col(text("text"))
            .col(big_integer("created_at"))
            .foreign_key(
                ForeignKey::create()
                    .from("agent_messages", "session_id")
                    .to("agent_sessions", "id")
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from("agent_messages", "run_id")
                    .to("agent_runs", "id")
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned(),
    )
    .await?;
    execute(
        db,
        Table::create()
            .table("agent_tool_calls")
            .if_not_exists()
            .col(big_integer("id").primary_key().auto_increment())
            .col(text("session_id"))
            .col(big_integer("run_id"))
            .col(big_integer("sequence"))
            .col(text("call_id"))
            .col(text("name"))
            .col(text("arguments_json"))
            .col(text("status"))
            .col(text("result_json").null())
            .col(text("result_content").null())
            .col(big_integer("created_at"))
            .col(big_integer("completed_at").null())
            .foreign_key(
                ForeignKey::create()
                    .from("agent_tool_calls", "session_id")
                    .to("agent_sessions", "id")
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from("agent_tool_calls", "run_id")
                    .to("agent_runs", "id")
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned(),
    )
    .await?;

    let indexes = [
        Index::create()
            .name("idx_agent_sessions_recent")
            .table("agent_sessions")
            .col("updated_at")
            .col("id")
            .to_owned(),
        Index::create()
            .name("uq_agent_runs_session_run")
            .table("agent_runs")
            .col("session_id")
            .col("run_number")
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_agent_runs_session")
            .table("agent_runs")
            .col("session_id")
            .to_owned(),
        Index::create()
            .name("uq_agent_messages_session_run_sequence")
            .table("agent_messages")
            .col("session_id")
            .col("run_id")
            .col("sequence")
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_agent_messages_run_sequence")
            .table("agent_messages")
            .col("run_id")
            .col("sequence")
            .to_owned(),
        Index::create()
            .name("uq_agent_tool_calls_session_run_sequence")
            .table("agent_tool_calls")
            .col("session_id")
            .col("run_id")
            .col("sequence")
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_agent_tool_calls_run_name")
            .table("agent_tool_calls")
            .col("run_id")
            .col("name")
            .to_owned(),
        Index::create()
            .name("idx_agent_tool_calls_session_call")
            .table("agent_tool_calls")
            .col("session_id")
            .col("call_id")
            .to_owned(),
    ];
    // Preserve declaration order without relying on a migration-manager dependency.
    execute_indexes(db, indexes.into_iter()).await
}

async fn create_provider_credentials<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    execute(
        db,
        Table::create()
            .table("agent_provider_credentials")
            .if_not_exists()
            .col(text("provider_id").primary_key())
            .col(text("auth_type"))
            .col(text("api_key").null())
            .col(text("access_token").null())
            .col(text("refresh_token").null())
            .col(big_integer("expires_at").null())
            .col(text("account_id").null())
            .col(big_integer("created_at"))
            .col(big_integer("updated_at"))
            .check(Expr::col(Alias::new("auth_type")).is_in(["oauth", "api_key"]))
            .to_owned(),
    )
    .await
}

async fn add_provider_base_url<C: ConnectionTrait>(db: &C) -> Result<(), DbErr> {
    execute(
        db,
        Table::alter()
            .table("agent_provider_credentials")
            .add_column(text("base_url").null())
            .to_owned(),
    )
    .await
}

async fn execute_indexes<C, I>(db: &C, indexes: I) -> Result<(), DbErr>
where
    C: ConnectionTrait,
    I: Iterator<Item = sea_orm::sea_query::IndexCreateStatement>,
{
    stream::iter(indexes.map(Ok::<_, DbErr>))
        .try_for_each(|statement| async { execute(db, statement).await })
        .await
}

async fn execute<C, S>(db: &C, statement: S) -> Result<(), DbErr>
where
    C: ConnectionTrait,
    S: StatementBuilder,
{
    db.execute(&statement).await.map(|_| ())
}

fn text(name: &'static str) -> ColumnDef {
    ColumnDef::new(name).text().not_null().to_owned()
}

fn big_integer(name: &'static str) -> ColumnDef {
    ColumnDef::new(name).big_integer().not_null().to_owned()
}

mod migration_history {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "agent_schema_migrations")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
