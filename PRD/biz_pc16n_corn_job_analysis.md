# 分析 biz_pc16n_corn_job 表关联关系

## 背景
用户需要了解 `biz_pc16n_corn_job` 表的关联关系，包括物理外键和逻辑关联。

## 调研结果
该表主要用于存储 PC16N 设备的定时任务（Cron Job）配置。通过对数据库模式的分析，发现以下关联关系：

### 1. 核心业务关联
| 关联字段 | 关联表 | 关联关系类型 | 说明 |
| :--- | :--- | :--- | :--- |
| `term_id` | `biz_term_info` | 1:N (逻辑) | 指向终端设备基础信息表，确定任务所属设备 |
| `tenant_id` | `sys_tenant` | 1:N (逻辑) | 指向租户表，属于标准多租户隔离字段 |
| `create_dept` | `sys_dept` | 1:N (逻辑) | 创建任务所在的部门 |
| `create_by` | `sys_user` | 1:N (逻辑) | 创建任务的用户 |
| `update_by` | `sys_user` | 1:N (逻辑) | 最后更新任务的用户 |

### 2. 同模块功能关联 (通过 term_id 隐式关联)
以下表均包含 `term_id` 字段，且属于同一功能模块（PC16N 设备管理）：
- `biz_pc16n_base_set`: PC16N 基本设置
- `biz_pc16n_protect_set`: PC16N 保护设置
- `biz_pc16n_monitor_data`: PC16N 监控数据
- `biz_pc16n_log`: PC16N 相关日志

### 3. 调度引擎关联
- **PowerJob 关联**: 系统中存在 `pj_job_info` 等 PowerJob 相关表。虽然数据库中没有直接的外键或 ID 记录，但在业务逻辑实现层，`biz_pc16n_corn_job` 的定时规则通常会被同步到调度引擎中执行。

## 验证计划
- [x] 通过 `INFORMATION_SCHEMA` 查询字段名匹配关系。
- [x] 描述相关表结构验证字段含义。
- [x] 检索同前缀表确认模块归属。
