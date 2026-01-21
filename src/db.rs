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
}
