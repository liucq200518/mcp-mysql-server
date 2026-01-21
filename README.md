# MySQL Model Context Protocol (MCP) Server

本项目是一个基于 Rust 开发的 MCP Server，用于连接 MySQL 数据库并为 MCP Client (如 Claude Desktop, IDE AI 插件等) 提供数据库查询与管理能力。

## ✨ 功能特性

- **多数据源支持**：通过配置文件同时管理多个 MySQL 连接。
- **配置热重载 (Hot Reload)**：修改并保存 `config.json` 后，服务端会自动更新连接池，**无需重启**。
- **双模通讯支持**：
    - **Stdio 模式**：通过标准输入输出进行 MCP 通讯，适用于本地 IDE 和桌面环境。
    - **HTTP/SSE 模式**：支持远程连接和跨网络通讯。
- **丰富的工具集**：
    - `list_connections`: 查看所有配置的连接。
    - `list_schemas`: 查看数据库列表。
    - `list_tables`: 查看表列表。
    - `describe_table`: 查看表结构。
    - `show_create_table`: 查看建表语句。
    - `execute_query`: 执行只读 SQL 查询。
    - `execute_ddl`: 执行 DDL 语句 (如 CREATE, ALTER) *[需用户确认]*。

## 🚀 快速开始

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

### 💡 配置文件自动重载

服务端启动后会持续观察配置文件。当您在 `config.json` 中添加、修改或删除数据库连接并保存后，控制台会显示 `Config file changed, reloading...`，变更将立即生效，Client 端无需重启。

## ⚠️ 安全说明

- **execute_ddl** 是危险操作，Client 端和模型应当谨慎使用。
- 建议配置数据库账号时遵循最小权限原则（如只分配 SELECT 权限给用于查询的连接）。

## 🛠️ 开发
- 依赖库：`tokio`, `sqlx`, `mcp-server`, `clap` 等。
- 运行测试：`cargo test`
