use crate::{mysql_pool_with_retry, MySqlConfig, StorageError};
use serde_json::{Map, Value};
use sqlx::{mysql::MySqlQueryResult, MySql, Pool, QueryBuilder};
use std::sync::Arc;

pub struct Repo {
    pool: Arc<Pool<MySql>>,
}

impl Repo {
    pub async fn new(config: &philand_configs::IdentityServiceConfig) -> Result<Self, StorageError> {
        let mysql_cfg = MySqlConfig {
            database_url: config.database_url.clone(),
            ..MySqlConfig::default()
        };
        let pool = mysql_pool_with_retry(&mysql_cfg).await?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    pub async fn new_repo(
        config: &philand_configs::IdentityServiceConfig,
    ) -> Result<Self, StorageError> {
        Self::new(config).await
    }

    pub fn from_pool(pool: Arc<Pool<MySql>>) -> Self {
        Self { pool }
    }

    pub async fn create(&self, table: &str, data: &Map<String, Value>) -> Result<u64, StorageError> {
        if data.is_empty() {
            return Err(StorageError::InvalidConfig("create data cannot be empty".to_string()));
        }

        let mut cols: Vec<&String> = data.keys().collect();
        cols.sort();
        let mut qb = QueryBuilder::<MySql>::new("INSERT INTO ");
        qb.push(quoted_table(table));
        qb.push(" (");
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push(quote_ident(c));
        }
        qb.push(") VALUES (");
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            push_json_bind(&mut qb, data.get(*c).expect("column exists"));
        }
        qb.push(")");

        let result: MySqlQueryResult = qb.build().execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn get(
        &self,
        table: &str,
        filters: &Map<String, Value>,
    ) -> Result<Option<Map<String, Value>>, StorageError> {
        let mut list = self.list_internal(table, filters, Some(1)).await?;
        Ok(list.pop())
    }

    pub async fn list(
        &self,
        table: &str,
        filters: &Map<String, Value>,
    ) -> Result<Vec<Map<String, Value>>, StorageError> {
        self.list_internal(table, filters, None).await
    }

    pub async fn update(
        &self,
        table: &str,
        data: &Map<String, Value>,
        filters: &Map<String, Value>,
    ) -> Result<u64, StorageError> {
        if data.is_empty() {
            return Err(StorageError::InvalidConfig("update data cannot be empty".to_string()));
        }
        if filters.is_empty() {
            return Err(StorageError::InvalidConfig("update filters cannot be empty".to_string()));
        }

        let mut set_cols: Vec<&String> = data.keys().collect();
        set_cols.sort();
        let mut where_cols: Vec<&String> = filters.keys().collect();
        where_cols.sort();

        let mut qb = QueryBuilder::<MySql>::new("UPDATE ");
        qb.push(quoted_table(table));
        qb.push(" SET ");
        for (i, c) in set_cols.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push(quote_ident(c));
            qb.push(" = ");
            push_json_bind(&mut qb, data.get(*c).expect("column exists"));
        }
        qb.push(" WHERE ");
        for (i, c) in where_cols.iter().enumerate() {
            if i > 0 {
                qb.push(" AND ");
            }
            qb.push(quote_ident(c));
            qb.push(" = ");
            push_json_bind(&mut qb, filters.get(*c).expect("column exists"));
        }

        let result: MySqlQueryResult = qb.build().execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn delete(&self, table: &str, filters: &Map<String, Value>) -> Result<u64, StorageError> {
        if filters.is_empty() {
            return Err(StorageError::InvalidConfig("delete filters cannot be empty".to_string()));
        }

        let mut cols: Vec<&String> = filters.keys().collect();
        cols.sort();

        let mut qb = QueryBuilder::<MySql>::new("DELETE FROM ");
        qb.push(quoted_table(table));
        qb.push(" WHERE ");
        for (i, c) in cols.iter().enumerate() {
            if i > 0 {
                qb.push(" AND ");
            }
            qb.push(quote_ident(c));
            qb.push(" = ");
            push_json_bind(&mut qb, filters.get(*c).expect("column exists"));
        }

        let result: MySqlQueryResult = qb.build().execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    pub async fn execute(&self, sql: &str) -> Result<u64, StorageError> {
        let result = sqlx::query(sql).execute(&*self.pool).await?;
        Ok(result.rows_affected())
    }

    async fn list_internal(
        &self,
        table: &str,
        filters: &Map<String, Value>,
        limit: Option<u64>,
    ) -> Result<Vec<Map<String, Value>>, StorageError> {
        let columns = self.table_columns(table).await?;
        if columns.is_empty() {
            return Ok(Vec::new());
        }

        let select_expr = format!("CAST({} AS CHAR)", json_object_expr(&columns));
        let mut qb = QueryBuilder::<MySql>::new("SELECT ");
        qb.push(select_expr);
        qb.push(" AS row_data FROM ");
        qb.push(quoted_table(table));

        if !filters.is_empty() {
            let mut cols: Vec<&String> = filters.keys().collect();
            cols.sort();
            qb.push(" WHERE ");
            for (i, c) in cols.iter().enumerate() {
                if i > 0 {
                    qb.push(" AND ");
                }
                qb.push(quote_ident(c));
                qb.push(" = ");
                push_json_bind(&mut qb, filters.get(*c).expect("column exists"));
            }
        }

        if let Some(l) = limit {
            qb.push(" LIMIT ");
            qb.push_bind(l);
        }

        let rows: Vec<(String,)> = qb.build_query_as().fetch_all(&*self.pool).await?;
        let mut out = Vec::with_capacity(rows.len());
        for (raw,) in rows {
            let value: Value = serde_json::from_str(&raw)
                .map_err(|e| StorageError::InvalidConfig(format!("invalid row json: {e}")))?;
            let map = value.as_object().cloned().ok_or_else(|| {
                StorageError::InvalidConfig("row json must be an object".to_string())
            })?;
            out.push(map);
        }

        Ok(out)
    }

    async fn table_columns(&self, full_name: &str) -> Result<Vec<ColumnMeta>, StorageError> {
        let mut parts = full_name.split('.');
        let schema = parts.next().unwrap_or("philand");
        let table = parts.next().unwrap_or(full_name);
        let rows: Vec<ColumnMeta> = sqlx::query_as(
            "SELECT COLUMN_NAME as column_name, DATA_TYPE as data_type FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(sqlx::FromRow)]
struct ColumnMeta {
    column_name: String,
    data_type: String,
}

fn table_name(full_name: &str) -> String {
    let mut parts = full_name.split('.');
    let db = parts.next().unwrap_or("philand");
    let table = parts.next().unwrap_or(full_name);
    format!("`{db}`.`{table}`")
}

fn quoted_table(full_name: &str) -> String {
    table_name(full_name)
}

fn quote_ident(name: &str) -> String {
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    format!("`{safe}`")
}

fn push_json_bind(qb: &mut QueryBuilder<'_, MySql>, v: &Value) {
    match v {
        Value::Null => {
            let none: Option<String> = None;
            qb.push_bind(none);
        }
        Value::Bool(b) => {
            qb.push_bind(*b);
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                qb.push_bind(i);
            } else if let Some(u) = n.as_u64() {
                qb.push_bind(u);
            } else if let Some(f) = n.as_f64() {
                qb.push_bind(f);
            } else {
                qb.push_bind(n.to_string());
            }
        }
        Value::String(s) => {
            qb.push_bind(s.clone());
        }
        Value::Array(_) | Value::Object(_) => {
            qb.push_bind(v.to_string());
        }
    }
}

fn json_object_expr(columns: &[ColumnMeta]) -> String {
    let mut parts = Vec::with_capacity(columns.len());
    for col in columns {
        let ident = quote_ident(&col.column_name);
        let value_expr = if matches!(
            col.data_type.as_str(),
            "timestamp" | "datetime" | "date" | "time"
        ) {
            format!(
                "CASE WHEN {ident} IS NULL THEN NULL ELSE DATE_FORMAT({ident}, '%Y-%m-%dT%H:%i:%sZ') END"
            )
        } else {
            ident
        };
        parts.push(format!("'{}', {}", col.column_name, value_expr));
    }
    format!("JSON_OBJECT({})", parts.join(", "))
}
