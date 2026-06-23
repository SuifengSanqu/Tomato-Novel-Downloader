# 用户指令记忆

本文件记录了用户的指令、偏好和教导，用于在未来的交互中提供参考。

## 格式

### 用户指令条目
用户指令条目应遵循以下格式：

[用户指令摘要]
- Date: [YYYY-MM-DD]
- Context: [提及的场景或时间]
- Instructions:
  - [用户教导或指示的内容，逐行描述]

### 项目知识条目
Agent 在任务执行过程中发现的条目应遵循以下格式：

[项目知识摘要]
- Date: [YYYY-MM-DD]
- Context: Agent 在执行 [具体任务描述] 时发现
- Category: [运维部署|构建方法|测试方法|排错调试|工作流协作|环境配置]
- Instructions:
  - [具体的知识点，逐行描述]

## 去重策略
- 添加新条目前，检查是否存在相似或相同的指令
- 若发现重复，跳过新条目或与已有条目合并
- 合并时，更新上下文或日期信息
- 这有助于避免冗余条目，保持记忆文件整洁

## 条目

### 环境编译注意事项
- Date: 2026-06-23
- Context: Agent 在重构多平台架构时发现
- Category: 构建方法
- Instructions:
  - 项目依赖外部 crate `tomato-novel-official-api`(路径 `/Tomato-Novel-Official-API`)，编译需要该 crate 真实实现，当前仅有 stub
  - 编译命令: `cargo check --features official-api --no-default-features`(禁用 TTS 避免 OpenSSL 依赖)
  - Rust edition 2024 需要 Rust >= 1.85，当前环境 1.96.0
  - 禁止使用 TTS 相关 feature: 使用 `--no-default-features` 然后单独启用 `official-api`

### 项目结构约定
- Date: 2026-06-23
- Context: Agent 在重构多平台架构时发现
- Category: 工作流协作
- Instructions:
  - `src/platform/` 为平台抽象层(跨平台统一接口 + 注册表)
  - 每个平台一个文件实现 `NovelPlatform` trait(fanqie.rs / qimao.rs)
  - `base_system/` 为基础设施层(配置/日志/路径/重试)，被所有上层模块依赖
  - `download/` 为下载流程编排层
  - `book_parser/` 为 EPUB/PDF/段评生成层
  - 外部 API crate 的 stub 应放在 `/Tomato-Novel-Official-API/src/lib.rs`
  - `#[cfg(feature = "official-api")]` 条件编译保护所有依赖该 crate 的代码

### 多平台架构设计原则
- Date: 2026-06-23
- Context: Agent 在设计多平台架构时确定
- Category: 构建方法
- Instructions:
  - `NovelId` 格式: `"platform:raw_id"`, 如 `"fanqie:7318247498772674083"`
  - 平台适配器中 `fetch_chapter` 返回 `ChapterContent` 携带 `ChapterState` 枚举(Ok/Locked/Failed)
  - 广告解锁的免费章节通过 Cookie 携带用户凭证访问(不视为付费破解)
  - 七猫适配器: API 优先 + HTML 解析回退策略
  - 平台注册表 `PlatformRegistry` 管理所有平台实例，支持多平台并行搜索聚合
