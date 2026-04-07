# Icebox

**Rust TUI 看板 + AI 侧边栏**

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | [中文](README.zh.md) | [Español](README.es.md)

基于Rust构建的终端看板，集成了由Anthropic API驱动的AI助手。使用Vim风格键绑定管理任务，通过每任务AI聊天进行对话，AI可直接操作看板和文件系统。

## 演示

### 01. 从安装到启动
> 通过Homebrew安装，一行命令即可打开看板。

![](videos/icebox-install.gif)

### 02. 看板与任务详情
> 在5列中管理任务，在侧边栏查看详细信息。

![](videos/icebox-kanban.gif)

### 03. 创建任务
> 按`n`键输入标题、标签和优先级，立即添加到看板。

![](videos/icebox-new-task.gif)

### 04. AI 侧边栏
> 每个任务都有独立的AI会话，用于对话和工具执行。

![](videos/icebox-ai-sidebar.gif)

### 05. AI创建任务
> 通过底部AI聊天创建任务并立即反映到看板上。

![](videos/icebox-ai-task.gif)

## 主要功能

- **5列看板** — Icebox, Emergency, In Progress, Testing, Complete
- **AI 侧边栏** — 每任务AI对话 + 流式响应
- **每任务会话** — 每个任务维护独立的AI对话历史，自动保存到磁盘
- **内置工具** — AI可执行Shell命令、读写文件、搜索代码、创建/更新任务
- **斜杠命令** — 18个命令用于看板管理、AI控制和认证
- **OAuth PKCE** — 通过claude.ai的浏览器登录
- **鼠标支持** — 点击选择、拖拽滚动、点击聚焦输入
- **文本选择** — 拖拽选择并复制AI响应
- **泳道** — 基于标签页的泳道过滤，`[`/`]`键切换
- **Notion风格链接** — 自动解析commit、PR、issue、branch引用
- **任务日期** — 每任务开始日期/截止日期管理
- **AI记忆** — 跨会话持久化的上下文记忆
- **任务存储** — 带YAML前言的Markdown文件 (`.icebox/tasks/`)

## 安装

### 快速安装

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

### Homebrew

```bash
brew tap SteelCrab/tap
brew install icebox
```

### 源码安装 (cargo install)

```bash
cargo install --git https://github.com/SteelCrab/icebox.git
```

### 源码构建

```bash
git clone https://github.com/SteelCrab/icebox.git
cd icebox
cargo build --release
cp target/release/icebox ~/.cargo/bin/
```

### 预构建二进制文件 (手动)

无法使用 `install.sh` 的环境，可从 [最新发布](https://github.com/SteelCrab/icebox/releases/latest)
直接下载适合您平台的二进制文件：

| 平台 | 架构 | 文件 |
|---|---|---|
| macOS | Apple Silicon (arm64) | `icebox-aarch64-apple-darwin.tar.gz` |
| macOS | Intel (x86_64) | `icebox-x86_64-apple-darwin.tar.gz` |
| Linux | x86_64 (glibc) | `icebox-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | x86_64 (musl / Alpine) | `icebox-x86_64-unknown-linux-musl.tar.gz` |
| Linux | aarch64 (glibc) | `icebox-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 (musl / Alpine) | `icebox-aarch64-unknown-linux-musl.tar.gz` |
| Linux | armv7 (Raspberry Pi 2/3) | `icebox-armv7-unknown-linux-gnueabihf.tar.gz` |

```bash
tar -xzf icebox-<target>.tar.gz
chmod +x icebox
mv icebox ~/.local/bin/    # 或 $PATH 中的任何目录
```

## 快速开始

### 前提条件

- Rust工具链 (edition 2024)
- Anthropic API密钥或Claude.ai账户

### 初始化工作区

```bash
icebox init              # 在当前目录初始化 .icebox/
icebox init ./my-board   # 在指定路径初始化
```

### 在指定路径运行

```bash
icebox                   # 在当前目录启动TUI
icebox ./my-board        # 在指定路径启动TUI
```

### 认证

设置API密钥（推荐）：

```bash
export ANTHROPIC_API_KEY=sk-ant-...
icebox
```

或OAuth登录：

```bash
icebox login           # 打开浏览器
icebox login --console # 控制台流程
icebox whoami          # 查看认证状态
```

## 键绑定

### Board模式

| 键 | 操作 |
|----|------|
| `h/l`, `←/→` | 移动列 |
| `j/k`, `↑/↓` | 移动任务 |
| `Enter` | 打开任务详情侧边栏 |
| `n` | 新建任务 |
| `d` | 删除任务 (y/Enter确认) |
| `>/<` | 将任务移到下一/上一列 |
| `[/]` | 泳道标签页切换 |
| `/` | 切换底部AI聊天面板 |
| `1/2` | 切换标签页 (Board / Memory) |
| `r` | 刷新 |
| `q`, `Ctrl+C` | 退出 |
| 鼠标点击 | 选择任务 + 打开详情 |
| 鼠标点击 (泳道栏) | 切换泳道 |
| 鼠标滚动 | 导航 |

### Detail模式（侧边栏）

| 键 | 操作 |
|----|------|
| `Tab`, `i` | 焦点切换: 详情 → 聊天 → 输入 |
| `e` | 编辑任务 (标题/正文) |
| `j/k` | 侧边栏滚动 |
| `>/<` | 任务列移动 |
| `Esc` | 取消焦点（分层）/ 返回看板 |
| `q` | 返回看板 |

### Edit模式

| 键 | 操作 |
|----|------|
| `Tab` | 标题 ↔ 正文 切换 |
| `Ctrl+S` | 保存 |
| `Esc` | 取消 |
| `Enter` | 下一字段 / 换行 |

### 底部AI聊天

| 键 | 操作 |
|----|------|
| `/` (Board) | 切换面板 |
| `Esc` | 取消焦点 |
| `Enter` | 发送消息 |
| `Tab` | 斜杠命令自动补全 |
| `Ctrl+↑/↓` | 调整面板大小 |

## 斜杠命令

| 类别 | 命令 | 说明 |
|------|------|------|
| **Board** | `/new <title>` | 创建任务 |
| | `/move <column>` | 移动任务 |
| | `/delete <id>` | 删除任务 |
| | `/search <query>` | 搜索任务 |
| | `/export` | 导出看板为Markdown |
| | `/diff` | Git diff |
| | `/swimlane [name \| clear]` | 设置泳道/列表 |
| **AI** | `/help` | 命令列表 |
| | `/status` | 会话状态 |
| | `/cost` | Token使用量 |
| | `/clear` | 清除对话 |
| | `/compact` | 压缩对话 |
| | `/model [name]` | 切换模型 |
| | `/remember <text>` | 保存AI上下文记忆 |
| | `/memory` | 记忆管理界面 |
| **Auth** | `/login` | OAuth登录 |
| | `/logout` | 登出 |
| **Session** | `/resume [id]` | 恢复会话 |

## 任务存储

任务以Markdown文件存储在 `.icebox/tasks/{id}.md`：

```markdown
---
id: "uuid"
title: "任务标题"
column: inprogress    # icebox | emergency | inprogress | testing | complete
priority: high        # low | medium | high | critical
tags: ["backend", "auth"]
swimlane: "backend"
start_date: "2026-04-01T00:00:00Z"
due_date: "2026-04-10T00:00:00Z"
created_at: "ISO8601"
updated_at: "ISO8601"
---

正文（Markdown）...

## 参考
- commit:abc1234
- PR#42
- branch:feature/auth
```

AI会话按任务自动保存在 `.icebox/sessions/{task_id}.json`。

### 工作区结构

```
.icebox/
├── .gitignore          # 自动生成（排除 sessions/, memory.json）
├── tasks/              # 任务Markdown文件
│   └── {id}.md
├── sessions/           # 每任务AI聊天会话
│   ├── __global__.json
│   └── {task_id}.json
└── memory.json         # AI记忆条目
```

## 架构

```
crates/
  icebox-cli/   # 主程序 — CLI子命令、TUI运行时
  tui/          # TUI — app, board, column, card, sidebar, input, layout, theme
  task/         # 领域 — Task, Column, Priority, frontmatter, TaskStore
  api/          # API — AnthropicClient, SSE流式传输, AuthMethod, 重试
  runtime/      # 运行时 — ConversationRuntime, Session, OAuth PKCE, UsageTracker
  tools/        # 12个工具 — bash, read/write_file, glob/grep_search, 看板 (list/create/update/move), 记忆
  commands/     # 18个斜杠命令 (Board, AI, Auth, Session)
```

## 内置AI工具

| 工具 | 说明 |
|------|------|
| `bash` | 执行Shell命令 |
| `read_file` | 读取文件内容 |
| `write_file` | 创建/写入文件 |
| `glob_search` | 通过Glob模式搜索文件 |
| `grep_search` | 通过正则表达式搜索文件内容 |
| `list_tasks` | 看板任务列表 |
| `create_task` | 新建任务 |
| `update_task` | 更新现有任务（标题、优先级、标签、泳道、日期、正文） |
| `move_task` | 任务列移动 |
| `save_memory` | 保存AI上下文记忆 |
| `list_memories` | 已保存记忆列表 |
| `delete_memory` | 删除记忆条目 |

## 环境变量

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `ANTHROPIC_API_KEY` | API密钥（推荐） | — |
| `ANTHROPIC_AUTH_TOKEN` | Bearer令牌 | — |
| `ANTHROPIC_BASE_URL` | API URL | `https://api.anthropic.com` |
| `ANTHROPIC_MODEL` | 模型 | `claude-sonnet-4-20250514` |
| `ICEBOX_CONFIG_HOME` | 配置目录 | `~/.icebox` |

## 许可证

根据 [Apache License, Version 2.0](LICENSE) 分发。
