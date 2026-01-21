# MySQL MCP Server 需求文档 (PRD)

## 1. 项目概述
本项目旨在开发一个基于 Model Context Protocol (MCP) 的服务端程序，使用 Rust 语言编写。该服务将充当 AI 模型与 MySQL 数据库之间的桥梁，允许 AI 通过 MCP 协议查询数据库元数据、表结构以及执行数据查询。

## 2. 核心功能
### 2.1 多数据源支持
- 支持通过配置文件定义多个 MySQL 数据库连接信息。
- 配置内容应包含：主机地址、端口、用户名、密码、数据库名等。

### 2.2 MCP 协议支持
- 实现 MCP 协议服务端，支持 stdio (命令行标准输入输出) 通信模式（或其他适合 CLI 运行的模式）。
- 提供 Tool 能力供 AI 调用。

### 2.3 数据库操作能力
- **元数据查询**：获取数据库中的表列表、表结构（字段、类型、注释等）。
- **数据查询**：执行 SQL 查询（需考虑只读安全性或限制）。

### 2.4 命令行界面 (CLI)
- 提供命令行启动入口。
- 支持指定配置文件路径。

## 3. 技术架构
### 3.1 开发语言
- Rust

### 3.2 关键依赖库 (暂定)
- **MCP SDK**: `mcp_server` 或 `mcp_protocol_sdk` (待验证最佳选择)
- **Database**: `sqlx` (MySQL support, async)
- **Async Runtime**: `tokio`
- **CLI**: `clap`
- **Config**: `serde`, `serde_json` 或 `toml`
- **Logging**: `tracing`, `tracing-subscriber`

## 4. 接口设计 (Tools)
AI 可使用的工具 (Tools) 预计包含：
- `list_connections()`: 列出配置文件中所有可用的数据库连接名称。
- `list_schemas(connection_name)`: 列出指定连接下的所有数据库 (Schemas)。
- `list_tables(connection_name, schema_name)`: 列出指定连接和数据库下的所有表。
- `describe_table(connection_name, schema_name, table_name)`: 查看指定表结构（字段、类型、索引、注释等）。
- `show_create_table(connection_name, schema_name, table_name)`: 获取表的建表语句 (SHOW CREATE TABLE)。
- `execute_query(connection_name, sql, schema_name)`: 在指定连接（和可选的数据库）上执行 SQL 查询 (主要用于 DQL)。
- `execute_ddl(connection_name, sql, schema_name)`: 执行 DDL 语句 (CREATE, ALTER, DROP 等) 以维护表结构。**注意：此工具涉及数据结构变更，具有潜在风险，建议 Tool 描述中明确增加 "Dangerous Operation" 提示，以便 Client 端（如果支持）能提示用户确认。**

## 5. 开发计划
1.  **项目初始化**：搭建 Rust 项目结构，配置依赖。
2.  **配置模块实现**：实现多数据源配置文件的读取与解析。
3.  **数据库连接池管理**：实现基于配置的动态连接池初始化。
4.  **MCP Server 集成**：集成 MCP SDK，搭建基础 Server 框架。
5.  **Tool 实现**：
    - 实现元数据查询逻辑。
    - 实现 SQL 执行逻辑。
6.  **CLI 封装**：完善命令行参数解析。
7.  **测试与验证**：编写单元测试，进行端到端测试。
