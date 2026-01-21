use crate::config::{AppConfig, ConnectionConfig};
use anyhow::{Context, Result};
use sqlx::{Column, MySql, Pool, Row, mysql::MySqlPoolOptions};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, info};

struct Inner {
    pools: HashMap<String, Pool<MySql>>,
    configs: HashMap<String, ConnectionConfig>,
}

#[derive(Clone)]
pub struct DbManager {
    inner: Arc<RwLock<Inner>>,
}

impl DbManager {
    pub async fn new(config: &AppConfig) -> Result<Self> {
        let mut pools = HashMap::new();
        let mut configs = HashMap::new();

        for (name, conf) in &config.connections {
            match Self::create_pool(name, conf) {
                Ok(pool) => {
                    pools.insert(name.clone(), pool);
                    configs.insert(name.clone(), conf.clone());
                }
                Err(e) => {
                    error!("Failed to initialize connection {}: {}", name, e);
                    // We continue even if one fails
                }
            }
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(Inner { pools, configs })),
        })
    }

    fn create_pool(name: &str, conf: &ConnectionConfig) -> Result<Pool<MySql>> {
        let url = format!(
            "mysql://{}:{}@{}:{}/{}",
            conf.username,
            conf.password.as_deref().unwrap_or(""),
            conf.host,
            conf.port,
            conf.database.as_deref().unwrap_or("")
        );

        info!("Initializing connection to database source: {}", name);
        // Use connect_lazy to avoid immediate failure if DB is down.
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect_lazy(&url)
            .context(format!("Failed to create pool for {}", name))?;

        // Spawn a background task to check connection health and retry
        let pool_clone = pool.clone();
        let name_clone = name.to_string();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                match pool_clone.acquire().await {
                    Ok(_) => {
                        info!("Successfully connected to database source: {}", name_clone);
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to connect to {}: {}. Retrying in 5s...",
                            name_clone,
                            e
                        );
                    }
                }
            }
        });

        Ok(pool)
    }

    pub fn update(&self, config: &AppConfig) -> Result<()> {
        let mut inner = self.inner.write().unwrap();

        // 1. Add or Update
        for (name, conf) in &config.connections {
            let needs_update = match inner.configs.get(name) {
                Some(existing_conf) => existing_conf != conf,
                None => true,
            };

            if needs_update {
                info!("Updating or adding connection: {}", name);
                match Self::create_pool(name, conf) {
                    Ok(pool) => {
                        inner.pools.insert(name.clone(), pool);
                        inner.configs.insert(name.clone(), conf.clone());
                    }
                    Err(e) => {
                        error!("Failed to update connection {}: {}", name, e);
                    }
                }
            }
        }

        // 2. Remove connections that are no longer in the config
        let new_names: Vec<String> = config.connections.keys().cloned().collect();
        inner.pools.retain(|k, _| {
            if !new_names.contains(k) {
                info!("Removing connection: {}", k);
                false
            } else {
                true
            }
        });
        inner.configs.retain(|k, _| new_names.contains(k));

        Ok(())
    }

    pub fn list_connections(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap();
        inner.pools.keys().cloned().collect()
    }

    fn get_pool(&self, name: &str) -> Result<Pool<MySql>> {
        let inner = self.inner.read().unwrap();
        inner
            .pools
            .get(name)
            .cloned()
            .context(format!("Connection '{}' not found", name))
    }

    pub async fn list_schemas(&self, connection_name: &str) -> Result<Vec<String>> {
        let pool = self.get_pool(connection_name)?;
        let rows = sqlx::query("SHOW DATABASES").fetch_all(&pool).await?;
        Ok(rows.iter().map(|row| row.get(0)).collect())
    }

    pub async fn list_tables(
        &self,
        connection_name: &str,
        schema_name: &str,
    ) -> Result<Vec<String>> {
        let pool = self.get_pool(connection_name)?;
        let query = format!("SHOW TABLES FROM `{}`", schema_name);
        let rows = sqlx::query(&query).fetch_all(&pool).await?;
        Ok(rows.iter().map(|row| row.get(0)).collect())
    }

    pub async fn describe_table(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = format!("DESCRIBE `{}`.`{}`", schema_name, table_name);
        let rows = sqlx::query(&query).fetch_all(&pool).await?;

        let mut output = String::new();
        output.push_str("Field | Type | Null | Key | Default | Extra\n");
        output.push_str("--- | --- | --- | --- | --- | ---\n");
        for row in rows {
            let field: String = row.try_get("Field").unwrap_or_default();
            let type_: String = row.try_get("Type").unwrap_or_default();
            let null: String = row.try_get("Null").unwrap_or_default();
            let key: String = row.try_get("Key").unwrap_or_default();
            let default: String = row
                .try_get("Default")
                .unwrap_or_else(|_| "NULL".to_string());
            let extra: String = row.try_get("Extra").unwrap_or_default();

            output.push_str(&format!(
                "{} | {} | {} | {} | {} | {}\n",
                field, type_, null, key, default, extra
            ));
        }
        Ok(output)
    }

    pub async fn show_create_table(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = format!("SHOW CREATE TABLE `{}`.`{}`", schema_name, table_name);
        let row = sqlx::query(&query).fetch_one(&pool).await?;
        let sql: String = row.try_get(1)?;
        Ok(sql)
    }

    pub async fn execute_query(
        &self,
        connection_name: &str,
        _schema_name: Option<&str>,
        sql: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let rows = sqlx::query(sql).fetch_all(&pool).await?;

        if rows.is_empty() {
            return Ok("No rows returned".to_string());
        }

        let mut results = Vec::new();
        for row in rows {
            let mut row_map = HashMap::new();
            for col in row.columns() {
                let name = col.name();
                let val: String = row
                    .try_get(name)
                    .unwrap_or_else(|_| "<blobs/unknown>".to_string());
                row_map.insert(name.to_string(), val);
            }
            results.push(row_map);
        }

        Ok(serde_json::to_string_pretty(&results)?)
    }

    pub async fn execute_ddl(
        &self,
        connection_name: &str,
        _schema_name: Option<&str>,
        sql: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let result = sqlx::query(sql).execute(&pool).await?;
        Ok(format!(
            "Execution successful. Rows affected: {}",
            result.rows_affected()
        ))
    }

    pub async fn get_table_preview(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
        limit: u32,
    ) -> Result<String> {
        let sql = format!(
            "SELECT * FROM `{}`.`{}` LIMIT {}",
            schema_name, table_name, limit
        );
        self.execute_query(connection_name, Some(schema_name), &sql)
            .await
    }

    pub async fn search_schema(&self, connection_name: &str, keyword: &str) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = "
            SELECT 'TABLE' as type, table_schema, table_name as name
            FROM information_schema.tables
            WHERE table_name LIKE ? OR table_comment LIKE ?
            UNION ALL
            SELECT 'COLUMN' as type, table_schema, CONCAT(table_name, '.', column_name) as name
            FROM information_schema.columns
            WHERE column_name LIKE ? OR column_comment LIKE ?
            LIMIT 100
        ";
        let pattern = format!("%{}%", keyword);
        let rows = sqlx::query(query)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .fetch_all(&pool)
            .await?;

        let mut output = String::new();
        output.push_str("Type | Schema | Name\n");
        output.push_str("--- | --- | ---\n");
        for row in rows {
            let type_: String = row.get(0);
            let schema: String = row.get(1);
            let name: String = row.get(2);
            output.push_str(&format!("{} | {} | {}\n", type_, schema, name));
        }
        Ok(output)
    }

    pub async fn get_indexes(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = format!("SHOW INDEX FROM `{}`.`{}`", schema_name, table_name);
        let rows = sqlx::query(&query).fetch_all(&pool).await?;

        let mut output = String::new();
        output.push_str(
            "Table | Non_unique | Key_name | Seq_in_index | Column_name | Index_type | Comment\n",
        );
        output.push_str("--- | --- | --- | --- | --- | --- | ---\n");
        for row in rows {
            let table: String = row.try_get("Table").unwrap_or_default();
            let non_unique: i32 = row.try_get("Non_unique").unwrap_or_default();
            let key_name: String = row.try_get("Key_name").unwrap_or_default();
            let seq: i32 = row.try_get("Seq_in_index").unwrap_or_default();
            let column: String = row.try_get("Column_name").unwrap_or_default();
            let index_type: String = row.try_get("Index_type").unwrap_or_default();
            let comment: String = row.try_get("Comment").unwrap_or_default();

            output.push_str(&format!(
                "{} | {} | {} | {} | {} | {} | {}\n",
                table, non_unique, key_name, seq, column, index_type, comment
            ));
        }
        Ok(output)
    }

    pub async fn get_foreign_keys(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = "
            SELECT 
                column_name, referenced_table_schema, referenced_table_name, referenced_column_name
            FROM information_schema.key_column_usage
            WHERE table_schema = ? AND table_name = ? AND referenced_table_name IS NOT NULL
        ";
        let rows = sqlx::query(query)
            .bind(schema_name)
            .bind(table_name)
            .fetch_all(&pool)
            .await?;

        let mut output = String::new();
        output.push_str("Column | Ref Schema | Ref Table | Ref Column\n");
        output.push_str("--- | --- | --- | ---\n");
        for row in rows {
            let col: String = row.get(0);
            let ref_schema: String = row.get(1);
            let ref_table: String = row.get(2);
            let ref_col: String = row.get(3);
            output.push_str(&format!(
                "{} | {} | {} | {}\n",
                col, ref_schema, ref_table, ref_col
            ));
        }
        Ok(output)
    }

    pub async fn get_table_stats(
        &self,
        connection_name: &str,
        schema_name: &str,
        table_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let query = "
            SELECT 
                table_rows, data_length, index_length, create_time, update_time, table_comment
            FROM information_schema.tables
            WHERE table_schema = ? AND table_name = ?
        ";
        let row = sqlx::query(query)
            .bind(schema_name)
            .bind(table_name)
            .fetch_one(&pool)
            .await?;

        let rows_count: Option<u64> = row.get(0);
        let data_len: Option<u64> = row.get(1);
        let index_len: Option<u64> = row.get(2);
        let create_time: Option<String> = row.try_get(3).ok();
        let update_time: Option<String> = row.try_get(4).ok();
        let comment: String = row.try_get(5).unwrap_or_default();

        let mut output = String::new();
        output.push_str(&format!("- **Rows**: {}\n", rows_count.unwrap_or(0)));
        output.push_str(&format!(
            "- **Data Size**: {} KB\n",
            data_len.unwrap_or(0) / 1024
        ));
        output.push_str(&format!(
            "- **Index Size**: {} KB\n",
            index_len.unwrap_or(0) / 1024
        ));
        output.push_str(&format!("- **Created**: {:?}\n", create_time));
        output.push_str(&format!("- **Updated**: {:?}\n", update_time));
        output.push_str(&format!("- **Comment**: {}\n", comment));
        Ok(output)
    }

    pub async fn dry_run_sql(&self, connection_name: &str, sql: &str) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let mut tx = pool.begin().await?;

        let result = sqlx::query(sql).execute(&mut *tx).await?;
        let rows_affected = result.rows_affected();

        tx.rollback().await?;

        Ok(format!(
            "Dry run successful (Transaction rolled back). Rows that would be affected: {}",
            rows_affected
        ))
    }

    pub async fn explain_query(&self, connection_name: &str, sql: &str) -> Result<String> {
        let explain_sql = format!("EXPLAIN {}", sql);
        self.execute_query(connection_name, None, &explain_sql)
            .await
    }

    pub async fn analyze_relationships(
        &self,
        connection_name: &str,
        schema_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;

        // 1. Get existing physical foreign keys to avoid duplicates
        let fk_query = "
            SELECT 
                table_name, column_name, referenced_table_name, referenced_column_name
            FROM information_schema.key_column_usage
            WHERE table_schema = ? AND referenced_table_name IS NOT NULL
        ";
        let physical_fks: Vec<(String, String, String, String)> =
            sqlx::query_as::<_, (String, String, String, String)>(fk_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await?;

        // 2. Get all columns that look like IDs (ending in _id or id case-insensitive)
        let col_query = "
            SELECT table_name, column_name
            FROM information_schema.columns
            WHERE table_schema = ? AND (column_name LIKE '%_id' OR column_name LIKE '%Id')
        ";
        let potential_links: Vec<(String, String)> =
            sqlx::query_as::<_, (String, String)>(col_query)
                .bind(schema_name)
                .fetch_all(&pool)
                .await?;

        // 3. Simple heuristic: if col_name is `[target_table]_id`, suggest relationship
        let mut suggestions = String::new();
        suggestions.push_str("Suggested Logical Relationships (Heuristic):\n\n");
        suggestions.push_str("Source Table | Source Column | Target Table | Note\n");
        suggestions.push_str("--- | --- | --- | ---\n");

        for (table, col) in potential_links {
            // Check if this is already a physical FK
            if physical_fks
                .iter()
                .any(|(t, c, _, _)| t == &table && c == &col)
            {
                continue;
            }

            // Extract target table name from column name
            let col_lower = col.to_lowercase();
            let target_prefix = if col_lower.ends_with("_id") {
                &col[..col.len() - 3]
            } else if col_lower.ends_with("id") {
                &col[..col.len() - 2]
            } else {
                continue;
            };

            // Heuristic match: find a table that matches the prefix
            suggestions.push_str(&format!(
                "{} | {} | {} (Guess) | Possible logical join\n",
                table, col, target_prefix
            ));
        }

        Ok(suggestions)
    }

    pub async fn scan_pii_columns(
        &self,
        connection_name: &str,
        schema_name: &str,
    ) -> Result<String> {
        let pool = self.get_pool(connection_name)?;
        let pii_keywords = vec![
            "%phone%",
            "%mobile%",
            "%email%",
            "%address%",
            "%password%",
            "%pwd%",
            "%id_card%",
            "%real_name%",
            "%card_no%",
            "%credit%",
        ];

        let mut query_builder = String::from(
            "
            SELECT table_name, column_name, column_comment
            FROM information_schema.columns
            WHERE table_schema = ? AND (
        ",
        );

        for i in 0..pii_keywords.len() {
            if i > 0 {
                query_builder.push_str(" OR ");
            }
            query_builder.push_str("column_name LIKE ? OR column_comment LIKE ?");
        }
        query_builder.push_str(")");

        let mut query = sqlx::query(&query_builder).bind(schema_name);
        for kw in pii_keywords {
            query = query.bind(kw).bind(kw);
        }

        let rows = query.fetch_all(&pool).await?;

        let mut output = String::new();
        output.push_str("Detected Potential PII Columns:\n\n");
        output.push_str("Table | Column | Comment\n");
        output.push_str("--- | --- | ---\n");
        for row in rows {
            let table: String = row.get(0);
            let col: String = row.get(1);
            let comment: String = row.get(2);
            output.push_str(&format!("{} | {} | {}\n", table, col, comment));
        }
        Ok(output)
    }

    pub async fn generate_schema_diff(
        &self,
        source_conn: &str,
        source_schema: &str,
        target_conn: &str,
        target_schema: &str,
    ) -> Result<String> {
        let source_pool = self.get_pool(source_conn)?;
        let target_pool = self.get_pool(target_conn)?;

        let table_query = "SELECT table_name FROM information_schema.tables WHERE table_schema = ?";

        let source_tables: Vec<String> = sqlx::query_scalar(table_query)
            .bind(source_schema)
            .fetch_all(&source_pool)
            .await?;
        let target_tables: Vec<String> = sqlx::query_scalar(table_query)
            .bind(target_schema)
            .fetch_all(&target_pool)
            .await?;

        let mut diff = String::new();
        diff.push_str(&format!(
            "Schema Diff: {} vs {}\n\n",
            source_conn, target_conn
        ));

        // 1. Missing tables in target
        for table in &source_tables {
            if !target_tables.contains(table) {
                diff.push_str(&format!(
                    "- [MISSING TABLE] `{}` exists in source but not in target.\n",
                    table
                ));
            }
        }

        // 2. Extra tables in target
        for table in &target_tables {
            if !source_tables.contains(table) {
                diff.push_str(&format!(
                    "- [EXTRA TABLE] `{}` exists in target but not in source.\n",
                    table
                ));
            }
        }

        // 3. Column differences in common tables
        let col_query = "SELECT column_name, column_type, is_nullable FROM information_schema.columns WHERE table_schema = ? AND table_name = ?";
        for table in &source_tables {
            if target_tables.contains(table) {
                let s_cols: Vec<(String, String, String)> = sqlx::query_as(col_query)
                    .bind(source_schema)
                    .bind(table)
                    .fetch_all(&source_pool)
                    .await?;
                let t_cols: Vec<(String, String, String)> = sqlx::query_as(col_query)
                    .bind(target_schema)
                    .bind(table)
                    .fetch_all(&target_pool)
                    .await?;

                for (s_name, s_type, s_null) in &s_cols {
                    if !t_cols.iter().any(|(t_name, _, _)| t_name == s_name) {
                        diff.push_str(&format!(
                            "- [MISSING COLUMN] Table `{}`: Column `{}` is missing in target.\n",
                            table, s_name
                        ));
                    } else {
                        let t_col_data = t_cols
                            .iter()
                            .find(|(t_name, _, _)| t_name == s_name)
                            .unwrap();
                        let (_, t_type, t_null) = t_col_data;
                        if s_type != t_type || s_null != t_null {
                            diff.push_str(&format!("- [DIFF COLUMN] Table `{}`: Column `{}` type/null mismatch (Source: {} {}, Target: {} {})\n", table, s_name, s_type, s_null, t_type, t_null));
                        }
                    }
                }
            }
        }

        if diff.lines().count() <= 2 {
            Ok("No major structural differences found between schemas.".to_string())
        } else {
            Ok(diff)
        }
    }
}
