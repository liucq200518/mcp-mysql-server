# MySQL Model Context Protocol (MCP) Server

本项目是一个基于 Rust 开发的 MCP Server，用于连接 MySQL 数据库并为 MCP Client (如 Claude Desktop, IDE AI 插件等) 提供数据库查询与管理能力。

## 功能特性

- **多数据源支持**：通过配置文件同时管理多个 MySQL 连接。
- **配置热重载 (Hot Reload)**：修改并保存 `config.json` 后，服务端会自动更新连接池，**无需重启**。
- **双模通讯支持**：支持本地 Stdio 排他模式和远程 HTTP/SSE 并发模式。

- **架构智能探索 (Advanced Meta)**: 
    - `get_table_preview`: 快速采样预览数据。
    - `search_schema`: 跨表跨列关键字搜索。
    - `get_indexes` / `get_foreign_keys`: 详尽的索引与物理外键查询。
    - `get_table_stats`: 获取表行数、存储尺寸及最后更新时间。
- **执行安全保障 (Safety)**: 
    - `dry_run_sql`: **无损试运行**。通过事务自动回滚技术预览 DML 变更影响。
- **性能与智能分析 (Intelligence)**: 
    - `explain_query`: 自动 SQL 执行计划分析。
    - `analyze_relationships`: **启发式关联分析**。自动识别表间逻辑关联。
    - `generate_schema_diff`: **跨环境架构对比**。找出数据源间的结构差异。
    - `scan_pii_columns`: **隐私合规扫描**。识别潜在的敏感数据列。


## MCP 工具能力全景图

| 类别 | 工具名称 (Tool Name) | 功能描述 | AI 最佳实践流程 / 使用建议 |
| :--- | :--- | :--- | :--- |
| **基础管理** | `list_connections` | 列出所有已配置的 MySQL 数据源 | 启动时的第一步，用于确认可用环境。 |
| | `list_schemas` | 查看选定连接下的所有数据库 | 切换业务库前调用。 |
| | `list_tables` | 查看数据库下的表列表 | 探索数据库结构的基础。 |
| **结构探索** | `describe_table` | 查看表字段定义 (类型、主键等) | 编写 DML 前必须确认字段精度和约束。 |
| | `show_create_table` | 获取建表原始 SQL | 理解索引、分区及复杂约束的终极手段。 |
| | `get_indexes` | 专门查询索引索引详情 | 性能调优前调用，确认覆盖索引。 |
| | `get_foreign_keys` | 查询物理外键关系 | 理解实体间硬性关联。 |
| **智能增强** | `get_table_preview` | 快速预览前 N 行采样数据 | 开发前理解业务数据形态，替代 `SELECT *`。 |
| | `search_schema` | 跨库搜索表、字段及注释 | 寻找特定业务逻辑（如“订单”、“用户”）所在的表。 |
| | `get_table_stats` | 获取表行数及存储统计 | 评估操作风险，避免对大表执行无索引查询。 |
| | `analyze_relationships` | 启发式逻辑关联关系分析 | 发现没有物理外键但存在逻辑关联的虚拟关系。 |
| **安全与性能** | `dry_run_sql` | **事务试运行 (自动回滚)** | **所有 UPDATE/DELETE 前的必经步骤**，验证影响行数。 |
| | `explain_query` | SQL 执行计划分析 (EXPLAIN) | 判断查询是否走索引，预防全表扫描导致的慢查询。 |
| | `generate_schema_diff` | 跨环境/库架构差异对比 | 部署前核对开发库与测试库的结构一致性。 |
| | `scan_pii_columns` | 敏感数据 (PII) 自动识别 | 隐私审计或导出数据前，自动标记脱敏列。 |
| **数据操作** | `execute_query` | 执行只读 DQL (SELECT) | 常规数据获取。 |
| | `execute_ddl` | 执行结构变更 (CREATE/ALTER) | 谨慎使用，通常需要人工二次确认。 |

---

## 快速开始

### 1. 编译
确保已安装 Rust 环境 (cargo)。
```bash
cargo build --release
```
编译产物位于 `target/release/mcp-mysql-server.exe`。

### 2. 配置
在同级目录或任意位置创建 `config.json` 文件：

```json
{
  "connections": {
    "local_test": {
      "host": "localhost",
      "port": 3306,
      "username": "root",
      "password": "password",
      "database": "test_db"
    },
    "prod_ref": {
        "host": "192.168.1.100",
        "port": 3306,
        "username": "readonly",
        "password": "secure",
        "database": "analytics"
    }
  }
}
```

### 3. 集成到 MCP Client (Stdio 模式)

以 Claude Desktop 为例，编辑配置文件 (如 `%APPDATA%\Claude\claude_desktop_config.json`)：

```json
{
  "mcpServers": {
    "mysql": {
      "command": "path/to/mcp-mysql-server.exe",
      "args": [
        "--config",
        "path/to/config.json"
      ]
    }
  }
}
```

### 4. 运行 HTTP/SSE 模式 (可选)

如果您希望以 HTTP 服务器模式运行以便远程连接：

```bash
mcp-mysql-server.exe --config config.json --port 3000
```

服务将监听 `http://0.0.0.0:3000`。
- MCP SSE 端点: `http://localhost:3000/sse`
- 消息发送端点: `http://localhost:3000/messages` (POST)

### 5. 集成到 MCP Client (SSE 模式)

如果客户端支持 SSE 传输协议（如某些自定义 MCP 客户端），配置如下：

```json
{
  "mcpServers": {
    "mysql-remote": {
      "url": "http://your-server-ip:3000/sse"
    }
  }
}
```

> [!NOTE]
> 目前 Claude Desktop 官方主要支持 stdio 模式。SSE 模式通常用于 Web 应用集成或支持网络传输的 MCP 代理软件。

## MCP 增强工具详解

### 1. 架构探索 (Exploration)
- **`get_table_preview`**: 采样数据。
    - 参数：`connection_name`, `schema_name`, `table_name`, `limit` (默认 10)。
- **`search_schema`**: 跨表/跨列关键字搜索。
    - 参数：`connection_name`, `keyword` (匹配表名、列名及注释)。
- **`get_indexes`**: 获取表索引详情。
- **`get_foreign_keys`**: 获取物理外键约束。
- **`get_table_stats`**: 获取表统计信息（行数估算、存储占用）。

### 2. 执行安全 (Safety)
- **`dry_run_sql`**: **强烈建议在执行 UPDATE/DELETE 前使用**。
    - 参数：`connection_name`, `sql`。
    - 原理：在开启的事务中运行 SQL 并捕获影响行数，随后立即回滚，不产生任何真实写入。

### 3. 性能与智能分析 (Intelligence)
- **`explain_query`**: 获取 SQL 执行计划。
- **`analyze_relationships`**: 启发式关联分析。
    - 原理：通过识别类似 `user_id` 的列名，自动推测表间可能的逻辑 Join 关系。
- **`generate_schema_diff`**: 两个数据源结构对比。
    - 参数：`source_connection`, `source_schema`, `target_connection`, `target_schema`。
- **`scan_pii_columns`**: 隐私数据预警。
    - 原理：根据敏感词库识别可能包含手机号、密码等字段。

---

## 场景化使用示例 (Prompt Examples)

### 场景 A：安全的线上数据修复
> **用户指令**: "帮我把 `order_db` 中 `status` 为 1 但 `payment_time` 为空的订单状态改为 0。"
>
> **AI 最佳实践流程**:
> 1. 调用 `get_table_stats` 评估表数据量。
> 2. 调用 `explain_query` 确认更新语句是否命中了索引，避免全表扫描。
> 3. 调用 `dry_run_sql` 确认语句语法正确且影响行数符合预期。
> 4. 最后调用 `execute_query` (DQL) 或提示用户使用其他工具执行真实更新。

### 场景 B：新环境架构审计
> **用户指令**: "对比 `dev_db` 和 `prod_db` 的结构差异，并找出可能包含敏感信息的列。"
>
> **AI 流程**:
> 1. 调用 `generate_schema_diff` 生成差异列表。
> 2. 调用 `scan_pii_columns` 标记出 `prod_db` 中的敏感列以进行合规核对。

---

### 场景 C：快速上手陌生库表 (Exploration)
> **用户指令**: "我不熟悉这个数据库，帮我找找哪里存储了用户信息，并给我看几条样板数据。"
>
> **AI 流程**:
> 1. 调用 `search_schema(keyword="user")` 在全库搜索匹配的表或字段。
> 2. 定位到 `sys_user_info` 表后，调用 `get_table_preview` 查看前 5 条真实数据，快速理解业务形态。
>
> ---
>
### 场景 D: 梳理业务关联逻辑 (Intelligence)
> **用户指令**: "分析一下 `order_main` 表和哪些表有关联，即使没有物理外键也帮我找出来。"
>
> **AI 流程**:
> 1. 调用 `get_foreign_keys` 检查数据库定义的硬性物理约束。
> 2. 调用 `analyze_relationships` 运行启发式分析，发现 `order_main.customer_id` 虽然没设外键，但在逻辑上高度关联 `customers.id`。
>
> ---
>
### 场景 E: 慢查询优化辅助 (Performance)
> **用户指令**: "这个查询语句执行很慢，帮我分析一下原因并看看现有的索引。"
>
> **AI 流程**:
> 1. 调用 `explain_query(sql="..." )` 查看执行计划，若发现 `type: ALL` 则存在全表扫描风险。
> 2. 调用 `get_indexes` 检查相关检索字段是否已建立索引，并据此给出扩充索引的建议。

---

## 配置文件自动重载

服务端启动后会持续观察配置文件。当您在 `config.json` 中添加、修改或删除数据库连接并保存后，控制台会显示 `Config file changed, reloading...`，变更将立即生效，Client 端无需重启。

## 安全说明

- **execute_ddl** 是危险操作，Client 端和模型应当谨慎使用。
- 建议配置数据库账号时遵循最小权限原则（如只分配 SELECT 权限给用于查询的连接）。

## 开发
- 依赖库：`tokio`, `sqlx`, `mcp-server`, `clap` 等。
- 运行测试：`cargo test`
