use crate::db::DbManager;
use anyhow::Result;
use mcp_server::router::RouterService;
use mcp_server::{ByteTransport, Router, Server};
use mcp_spec::{
    content::Content,
    handler::{PromptError, ResourceError, ToolError},
    prompt::Prompt,
    protocol::{ServerCapabilities, ToolsCapability},
    tool::Tool,
};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, stdin, stdout};

// --- App Router Implementation ---

#[derive(Clone)]
pub struct AppRouter {
    db: Arc<DbManager>,
}

impl AppRouter {
    pub fn new(db: Arc<DbManager>) -> Self {
        Self { db }
    }
}

impl Router for AppRouter {
    fn name(&self) -> String {
        "mcp-mysql-server".to_string()
    }

    fn instructions(&self) -> String {
        "A MySQL MCP server that provides database access.".to_string()
    }

    fn capabilities(&self) -> ServerCapabilities {
        ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(false),
            }),
            prompts: None,
            resources: None,
        }
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "list_connections".to_string(),
                description: "List all available database connections".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "list_schemas".to_string(),
                description: "List schemas in a connection".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string", "description": "The name of the connection" }
                    },
                    "required": ["connection_name"]
                }),
            },
            Tool {
                name: "list_tables".to_string(),
                description: "List tables in a schema".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name"]
                }),
            },
            Tool {
                name: "describe_table".to_string(),
                description: "Describe table structure".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "show_create_table".to_string(),
                description: "Show create table statement".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "execute_query".to_string(),
                description: "Execute a read-only SQL query".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "sql": { "type": "string" }
                    },
                    "required": ["connection_name", "sql"]
                }),
            },
            Tool {
                name: "execute_ddl".to_string(),
                description: "Execute a DDL SQL query (Warning: Destructive operation)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "sql": { "type": "string" }
                    },
                    "required": ["connection_name", "sql"]
                }),
            },
            Tool {
                name: "get_table_preview".to_string(),
                description: "Quickly preview data from a table".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" },
                        "limit": { "type": "integer", "default": 10 }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "search_schema".to_string(),
                description: "Search for tables or columns by keyword across the schema"
                    .to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "keyword": { "type": "string" }
                    },
                    "required": ["connection_name", "keyword"]
                }),
            },
            Tool {
                name: "get_indexes".to_string(),
                description: "Get index information for a table".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "get_foreign_keys".to_string(),
                description: "Get foreign key relationships for a table".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "get_table_stats".to_string(),
                description: "Get table statistics like row count and size".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" },
                        "table_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name", "table_name"]
                }),
            },
            Tool {
                name: "dry_run_sql".to_string(),
                description: "Execute a DML SQL query (INSERT/UPDATE/DELETE) in a transaction and rollback to see effect".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "sql": { "type": "string" }
                    },
                    "required": ["connection_name", "sql"]
                }),
            },
            Tool {
                name: "explain_query".to_string(),
                description: "Execute EXPLAIN on a SQL query to analyze performance".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "sql": { "type": "string" }
                    },
                    "required": ["connection_name", "sql"]
                }),
            },
            Tool {
                name: "analyze_relationships".to_string(),
                description: "Analyze logical relationships and suggested joins based on naming conventions".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name"]
                }),
            },
            Tool {
                name: "scan_pii_columns".to_string(),
                description: "Scan schema for potential PII (Personally Identifiable Information) columns".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_name": { "type": "string" },
                        "schema_name": { "type": "string" }
                    },
                    "required": ["connection_name", "schema_name"]
                }),
            },
            Tool {
                name: "generate_schema_diff".to_string(),
                description: "Generate structural diff between two different data sources/schemas".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_connection": { "type": "string" },
                        "source_schema": { "type": "string" },
                        "target_connection": { "type": "string" },
                        "target_schema": { "type": "string" }
                    },
                    "required": ["source_connection", "source_schema", "target_connection", "target_schema"]
                }),
            },
        ]
    }

    fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        let db = self.db.clone();
        let tool_name = tool_name.to_string();

        Box::pin(async move {
            match tool_name.as_str() {
                "list_connections" => {
                    let conns = db.list_connections();
                    let json = serde_json::to_string_pretty(&conns)
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(json)])
                }
                "list_schemas" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schemas = db
                        .list_schemas(conn)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(
                        serde_json::to_string_pretty(&schemas).unwrap(),
                    )])
                }
                "list_tables" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let tables = db
                        .list_tables(conn, schema)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(
                        serde_json::to_string_pretty(&tables).unwrap(),
                    )])
                }
                "describe_table" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let desc = db
                        .describe_table(conn, schema, table)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(desc)])
                }
                "show_create_table" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let create = db
                        .show_create_table(conn, schema, table)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(create)])
                }
                "execute_query" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let sql = arguments
                        .get("sql")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing sql".into()))?;
                    let schema = arguments.get("schema_name").and_then(|v| v.as_str());
                    let result = db
                        .execute_query(conn, schema, sql)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "execute_ddl" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let sql = arguments
                        .get("sql")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing sql".into()))?;
                    let schema = arguments.get("schema_name").and_then(|v| v.as_str());
                    let result = db
                        .execute_ddl(conn, schema, sql)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "get_table_preview" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let limit = arguments
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(10) as u32;
                    let preview = db
                        .get_table_preview(conn, schema, table, limit)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(preview)])
                }
                "search_schema" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let keyword = arguments
                        .get("keyword")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing keyword".into()))?;
                    let result = db
                        .search_schema(conn, keyword)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "get_indexes" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let result = db
                        .get_indexes(conn, schema, table)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "get_foreign_keys" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let result = db
                        .get_foreign_keys(conn, schema, table)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "get_table_stats" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let table = arguments
                        .get("table_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing table_name".into()))?;
                    let result = db
                        .get_table_stats(conn, schema, table)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "dry_run_sql" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let sql = arguments
                        .get("sql")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing sql".into()))?;
                    let result = db
                        .dry_run_sql(conn, sql)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "explain_query" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let sql = arguments
                        .get("sql")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing sql".into()))?;
                    let result = db
                        .explain_query(conn, sql)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "analyze_relationships" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let result = db
                        .analyze_relationships(conn, schema)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "scan_pii_columns" => {
                    let conn = arguments
                        .get("connection_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing connection_name".into(),
                        ))?;
                    let schema = arguments
                        .get("schema_name")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing schema_name".into()))?;
                    let result = db
                        .scan_pii_columns(conn, schema)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                "generate_schema_diff" => {
                    let s_conn = arguments
                        .get("source_connection")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing source_connection".into(),
                        ))?;
                    let s_schema = arguments
                        .get("source_schema")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing source_schema".into()))?;
                    let t_conn = arguments
                        .get("target_connection")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters(
                            "Missing target_connection".into(),
                        ))?;
                    let t_schema = arguments
                        .get("target_schema")
                        .and_then(|v| v.as_str())
                        .ok_or(ToolError::InvalidParameters("Missing target_schema".into()))?;
                    let result = db
                        .generate_schema_diff(s_conn, s_schema, t_conn, t_schema)
                        .await
                        .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
                    Ok(vec![Content::text(result)])
                }
                _ => Err(ToolError::NotFound(format!("Tool {} not found", tool_name))),
            }
        })
    }

    fn list_resources(&self) -> Vec<mcp_spec::resource::Resource> {
        vec![]
    }

    fn read_resource(
        &self,
        _uri: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        Box::pin(async { Err(ResourceError::NotFound("Resources not implemented".into())) })
    }

    fn list_prompts(&self) -> Vec<Prompt> {
        vec![]
    }

    fn get_prompt(
        &self,
        _prompt_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, PromptError>> + Send + 'static>> {
        Box::pin(async { Err(PromptError::InternalError("Prompts not implemented".into())) })
    }
}

// --- STDIO Server ---

pub async fn start_stdio_server(db: DbManager) -> Result<()> {
    let transport = ByteTransport::new(stdin(), stdout());
    let db = Arc::new(db);
    let router = AppRouter::new(db);
    let service = RouterService(router);
    let server = Server::new(service);
    server.run(transport).await?;
    Ok(())
}

// --- HTTP Server (Salvo) ---

// --- HTTP Server (Salvo) ---

use salvo::prelude::*;
use std::io::Cursor;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

// Shared state to bridge HTTP and MCP Server
struct ServerState {
    input_tx: mpsc::Sender<Vec<u8>>,
    output_rx: Arc<Mutex<mpsc::Receiver<String>>>,
}

struct StateInjector {
    state: Arc<ServerState>,
}

#[async_trait]
impl Handler for StateInjector {
    async fn handle(
        &self,
        _req: &mut Request,
        depot: &mut Depot,
        _res: &mut Response,
        _ctrl: &mut FlowCtrl,
    ) {
        depot.inject(self.state.clone());
    }
}

#[handler]
async fn handle_sse(res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<ServerState>>().unwrap();
    let rx_clone = state.output_rx.clone();

    let stream = async_stream::stream! {
        loop {
            let msg = {
                 let mut rx = rx_clone.lock().await;
                 rx.recv().await
            };

            match msg {
                Some(msg) => {
                    yield Ok::<_, salvo::Error>(salvo::sse::SseEvent::default().text(msg));
                }
                None => {
                    break;
                }
            }
        }
    };

    salvo::sse::stream(res, stream);
}

#[handler]
async fn handle_post(req: &mut Request, depot: &mut Depot) -> String {
    let state = depot.obtain::<Arc<ServerState>>().unwrap();
    let body = req.payload().await.map(|b| b.to_vec()).unwrap_or_default();

    let mut bytes = body.to_vec();
    if !bytes.ends_with(b"\n") {
        bytes.push(b'\n');
    }

    if let Err(_) = state.input_tx.send(bytes).await {
        return "Server error: channel closed".to_string();
    }

    "Accepted".to_string()
}

pub async fn start_http_server(db: DbManager, port: u16) -> Result<()> {
    let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>(32);
    let (output_tx, output_rx) = mpsc::channel::<String>(32);

    let db = Arc::new(db);
    let router = AppRouter::new(db);
    let service = RouterService(router);
    let server = Server::new(service);

    let reader = ChannelReader {
        rx: input_rx,
        buffer: Cursor::new(Vec::new()),
    };

    let writer = ChannelWriter { tx: output_tx };

    let transport = ByteTransport::new(reader, writer);

    // Spawn a dedicated thread with a new runtime because mcp-server future is !Send
    // (likely due to tracing::instrument span holding across awaits)
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            if let Err(e) = server.run(transport).await {
                tracing::error!("MCP Server error: {}", e);
            }
        });
    });

    let state = Arc::new(ServerState {
        input_tx,
        output_rx: Arc::new(Mutex::new(output_rx)),
    });

    let router = salvo::Router::new()
        .hoop(StateInjector { state })
        .push(salvo::Router::with_path("sse").get(handle_sse))
        .push(salvo::Router::with_path("messages").post(handle_post));

    tracing::info!("Starting MCP HTTP Server on http://0.0.0.0:{}", port);
    let acceptor = TcpListener::new(format!("0.0.0.0:{}", port)).bind().await;
    salvo::Server::new(acceptor).serve(router).await;

    Ok(())
}

struct ChannelReader {
    rx: mpsc::Receiver<Vec<u8>>,
    buffer: Cursor<Vec<u8>>,
}

impl AsyncRead for ChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if self.buffer.position() < self.buffer.get_ref().len() as u64 {
            let n = std::io::Read::read(&mut self.buffer, buf.initialize_unfilled())?;
            buf.advance(n);
            return std::task::Poll::Ready(Ok(()));
        }

        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(data)) => {
                self.buffer = Cursor::new(data);
                self.poll_read(cx, buf)
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(Ok(())),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

struct ChannelWriter {
    tx: mpsc::Sender<String>,
}

impl AsyncWrite for ChannelWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let s = String::from_utf8_lossy(buf).to_string();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(s).await;
        });
        std::task::Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
