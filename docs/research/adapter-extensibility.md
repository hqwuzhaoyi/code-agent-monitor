# Adapter 可扩展性分析报告

## 概述

本报告分析 CAM 当前架构对新 CLI 工具支持的可扩展性，评估 trait 设计、GenericAdapter 作用、新增 CLI 的工作量，以及插件化/配置驱动的可能性。

## 1. 当前架构分析

### 1.1 AgentType 枚举设计

当前 `AgentType` 定义在 `src/agent_mod/manager.rs`:

```rust
pub enum AgentType {
    Claude,
    OpenCode,
    Codex,
    GeminiCli,
    MistralVibe,
    Mock,     // 用于测试
    Unknown,  // 未知类型
}
```

**优点**:
- 类型安全，编译时检查
- 序列化/反序列化支持完善
- 支持多种别名解析（如 "claude-code" → Claude）

**局限**:
- 新增 CLI 需要修改源码
- 需要重新编译发布
- 社区贡献需要 PR 合并流程

### 1.2 状态检测策略

当前使用 **AI 驱动的统一检测**，而非硬编码模式：

```rust
// src/infra/input.rs
pub fn detect_immediate(&self, output: &str) -> InputWaitResult {
    match is_agent_processing(&context) {
        AgentStatus::DecisionRequired => ...
        AgentStatus::WaitingForInput => ...
        AgentStatus::Processing | AgentStatus::Running => ...
        AgentStatus::Unknown => ...
    }
}
```

**优点**:
- 无需为每个 CLI 编写特定模式
- 自动适应新工具的 UI 变化
- 符合 CLAUDE.md "避免硬编码 AI 工具特定模式" 原则

**局限**:
- 依赖 Haiku API 可用性
- 对非标准 TUI 可能判断不准确
- API 调用有延迟和成本

### 1.3 事件处理架构

当前事件流：

```
终端输出 → InputWaitDetector (AI) → WatchEvent → NotificationEvent → Dispatcher
```

事件类型定义在 `src/notification/event.rs`，与具体 CLI 解耦。

## 2. Trait 设计灵活性评估

### 2.1 当前隐式 Trait

虽然没有显式定义 `AgentAdapter` trait，但代码中存在隐式的适配器模式：

```rust
// src/agent_mod/manager.rs
fn get_agent_command(&self, agent_type: &AgentType, resume_session: Option<&str>) -> String {
    match agent_type {
        AgentType::Claude => { ... }
        AgentType::OpenCode => "opencode".to_string(),
        AgentType::Codex => "codex".to_string(),
        // ...
    }
}
```

### 2.2 建议的 Trait 设计

```rust
pub trait AgentAdapter: Send + Sync {
    /// 获取 Agent 类型标识
    fn agent_type(&self) -> &str;

    /// 获取启动命令
    fn get_start_command(&self, resume_session: Option<&str>) -> String;

    /// 获取配置文件路径
    fn config_paths(&self) -> Vec<PathBuf>;

    /// 获取会话存储路径
    fn session_storage_path(&self) -> Option<PathBuf>;

    /// 获取日志路径
    fn log_path(&self) -> Option<PathBuf>;

    /// 进程识别模式（用于扫描）
    fn process_patterns(&self) -> Vec<&str>;

    /// 是否支持 hooks/plugins
    fn supports_hooks(&self) -> bool;

    /// 获取 hook 配置路径
    fn hook_config_path(&self) -> Option<PathBuf>;
}
```

**灵活性评估**: ⭐⭐⭐⭐ (4/5)

- 可以通过 trait object 实现动态分发
- 支持运行时注册新适配器
- 但需要重构现有代码

## 3. GenericAdapter 的作用和局限性

### 3.1 当前 "Generic" 处理

当前通过 `Unknown` 类型和 AI 检测实现通用支持：

```rust
AgentType::Unknown => "echo 'Unknown agent type'".to_string(),
```

结合 AI 状态检测，理论上可以监控任何终端程序。

### 3.2 GenericAdapter 设计建议

```rust
pub struct GenericAdapter {
    /// CLI 名称
    name: String,
    /// 启动命令
    command: String,
    /// 配置路径（可选）
    config_path: Option<PathBuf>,
    /// 进程匹配模式
    process_pattern: String,
}

impl GenericAdapter {
    pub fn from_config(config: &GenericAdapterConfig) -> Self { ... }
}
```

**作用**:
- 支持用户自定义 CLI 而无需修改源码
- 提供合理的默认行为
- 利用 AI 检测实现状态判断

**局限性**:
- 无法利用 CLI 特有的 hooks/events
- 状态检测完全依赖 AI（可能不准确）
- 无法获取结构化事件（如工具调用详情）

## 4. 新增 CLI 支持的工作量估算

### 4.1 当前方式（修改源码）

| 步骤 | 工作量 | 说明 |
|------|--------|------|
| 添加 AgentType 变体 | 5 分钟 | 修改 enum 和 FromStr |
| 添加启动命令 | 5 分钟 | 修改 get_agent_command |
| 添加配置路径 | 10 分钟 | 如果需要读取 CLI 配置 |
| 添加 hooks 集成 | 2-8 小时 | 如果 CLI 支持 hooks |
| 测试 | 1-2 小时 | 端到端测试 |
| **总计** | **3-10 小时** | 取决于 hooks 复杂度 |

### 4.2 配置驱动方式（建议）

| 步骤 | 工作量 | 说明 |
|------|--------|------|
| 编写配置文件 | 10 分钟 | TOML/JSON 配置 |
| 测试 | 30 分钟 | 验证基本功能 |
| **总计** | **40 分钟** | 无需编译 |

### 4.3 工作量对比

```
当前方式:  ████████████████████████████████ (3-10 小时)
配置驱动:  ████ (40 分钟)
```

## 5. 插件化/动态加载的可能性

### 5.1 Rust 动态加载方案

**方案 A: 动态库 (libloading)**

```rust
// 加载外部 .so/.dylib
let lib = Library::new("adapters/gemini.so")?;
let adapter: Symbol<fn() -> Box<dyn AgentAdapter>> = lib.get(b"create_adapter")?;
```

**优点**: 真正的插件系统
**缺点**:
- ABI 兼容性问题
- 安全风险
- 跨平台复杂

**方案 B: WASM 插件**

```rust
// 使用 wasmtime 加载 WASM 模块
let module = Module::from_file(&engine, "adapters/gemini.wasm")?;
```

**优点**: 沙箱安全，跨平台
**缺点**:
- 性能开销
- 生态不成熟
- 开发复杂度高

**方案 C: 配置驱动 + 脚本扩展（推荐）**

```toml
# ~/.config/code-agent-monitor/adapters/gemini.toml
[adapter]
name = "gemini-cli"
command = "gemini"
aliases = ["gemini", "gemini-cli", "geminicli"]

[paths]
config = "~/.config/gemini/"
sessions = "~/.config/gemini/sessions/"

[process]
patterns = ["gemini", "gemini-cli"]

[hooks]
# 可选：指定 hook 脚本
on_start = "~/.config/code-agent-monitor/hooks/gemini-start.sh"
```

**优点**:
- 无需重新编译
- 用户友好
- 安全（无代码执行）
- 可与 AI 检测结合

**缺点**:
- 功能受限于配置项
- 复杂逻辑需要脚本

### 5.2 推荐方案

采用 **分层扩展架构**:

```
┌─────────────────────────────────────────────────────────┐
│                    内置适配器 (Rust)                      │
│  Claude, OpenCode, Codex - 完整功能，hooks 集成          │
├─────────────────────────────────────────────────────────┤
│                  配置驱动适配器 (TOML)                    │
│  用户自定义 CLI - 基本功能，AI 状态检测                   │
├─────────────────────────────────────────────────────────┤
│                  脚本扩展 (Shell/Python)                  │
│  自定义 hooks - 高级用户，特殊需求                        │
└─────────────────────────────────────────────────────────┘
```

## 6. 配置驱动 vs 代码驱动的权衡

### 6.1 对比分析

| 维度 | 代码驱动 | 配置驱动 |
|------|----------|----------|
| **类型安全** | ✅ 编译时检查 | ❌ 运行时验证 |
| **性能** | ✅ 零开销 | ⚠️ 解析开销 |
| **灵活性** | ❌ 需重新编译 | ✅ 热加载 |
| **用户友好** | ❌ 需要 Rust 知识 | ✅ 只需编辑配置 |
| **功能完整性** | ✅ 完整 API 访问 | ⚠️ 受限于配置项 |
| **维护成本** | ⚠️ 每个 CLI 需维护 | ✅ 用户自维护 |
| **社区贡献** | ❌ PR 流程 | ✅ 分享配置文件 |

### 6.2 建议策略

**核心 CLI（代码驱动）**:
- Claude Code, OpenCode, Codex
- 需要深度集成 hooks/events
- 由项目维护者维护

**扩展 CLI（配置驱动）**:
- Gemini CLI, Mistral Vibe, 其他新兴工具
- 基本监控功能
- 用户/社区维护

## 7. 社区贡献友好度

### 7.1 当前状态

| 方面 | 评分 | 说明 |
|------|------|------|
| 文档 | ⭐⭐⭐ | CLAUDE.md 有开发指南 |
| 代码结构 | ⭐⭐⭐⭐ | 模块化清晰 |
| 贡献门槛 | ⭐⭐ | 需要 Rust 知识 |
| 测试覆盖 | ⭐⭐⭐ | 有单元测试 |
| CI/CD | ⭐⭐⭐ | 基本流程 |

### 7.2 改进建议

1. **降低贡献门槛**
   - 提供配置驱动的适配器系统
   - 创建 `adapters/` 目录存放社区配置
   - 编写适配器开发指南

2. **标准化接口**
   - 定义 `AgentAdapter` trait
   - 提供适配器模板
   - 文档化扩展点

3. **社区配置仓库**
   - 创建 `cam-adapters` 仓库
   - 收集社区贡献的配置
   - 提供一键安装脚本

## 8. 实施建议

### 8.1 短期（1-2 周）

1. **定义 AgentAdapter trait**
   - 抽象当前隐式接口
   - 保持向后兼容

2. **实现配置加载**
   - 支持 `~/.config/code-agent-monitor/adapters/*.toml`
   - 运行时注册适配器

### 8.2 中期（1 个月）

1. **重构现有适配器**
   - Claude, OpenCode, Codex 实现 trait
   - 提取公共逻辑到 GenericAdapter

2. **完善文档**
   - 适配器开发指南
   - 配置文件格式说明
   - 示例配置

### 8.3 长期（3 个月）

1. **社区生态**
   - 建立适配器仓库
   - 贡献指南
   - 自动化测试

2. **高级扩展**
   - 脚本 hooks 支持
   - 事件转换层
   - 自定义状态检测

## 9. 结论

### 9.1 当前架构评估

| 维度 | 评分 | 说明 |
|------|------|------|
| **Trait 灵活性** | ⭐⭐⭐ | 隐式存在，需显式化 |
| **GenericAdapter** | ⭐⭐⭐⭐ | AI 检测提供良好通用性 |
| **新增工作量** | ⭐⭐ | 需修改源码，门槛较高 |
| **插件化潜力** | ⭐⭐⭐⭐ | 配置驱动方案可行 |
| **社区友好度** | ⭐⭐⭐ | 有改进空间 |

### 9.2 核心建议

1. **采用分层扩展架构**
   - 内置适配器（代码）+ 配置适配器（TOML）+ 脚本扩展

2. **利用 AI 检测优势**
   - 作为通用后备方案
   - 减少特定 CLI 的硬编码

3. **降低贡献门槛**
   - 配置驱动优先
   - 完善文档和示例

4. **保持核心简洁**
   - 只内置主流 CLI
   - 其他通过配置扩展

### 9.3 风险提示

- AI 检测依赖 API 可用性，需要离线后备方案
- 配置驱动无法实现深度集成（如 hooks 事件）
- 过度抽象可能增加维护复杂度

---

*报告生成时间: 2026-02-24*
*分析基于 CAM 当前 main 分支代码*
