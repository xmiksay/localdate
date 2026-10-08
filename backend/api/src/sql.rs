//! Raw SQL for what SeaORM's query builder does not express well (upserts, `unnest`, conditional
//! deletes).

use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement, Value};

/// Runs one Postgres statement with positional `$n` parameters; returns the affected row count.
pub async fn exec(db: &impl ConnectionTrait, sql: &str, values: Vec<Value>) -> Result<u64, DbErr> {
    let stmt = Statement::from_sql_and_values(DbBackend::Postgres, sql, values);
    Ok(db.execute(stmt).await?.rows_affected())
}
