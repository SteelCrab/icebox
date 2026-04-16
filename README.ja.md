# Icebox

**Rust TUI カンバンボード + AI サイドバー**

[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | [中文](README.zh.md) | [Español](README.es.md)

Rustで構築されたターミナルベースのカンバンボードで、Anthropic APIを活用したAIアシスタントが統合されています。Vimスタイルのキーバインドでタスクを管理し、タスクごとのAIチャットで対話し、AIがボードとファイルシステムを直接操作できます。

## デモ

### 01. インストールから起動まで
> Homebrewでインストールし、icebox 1コマンドでボードが開きます。

![](videos/icebox-install.gif)

### 02. カンバンとタスク詳細
> 5つのカラムでタスクを管理し、サイドバーで詳細を確認します。

![](videos/icebox-kanban.gif)

### 03. タスク作成
> `n`キーでタイトル、タグ、優先度を入力し、即座にボードに追加します。

![](videos/icebox-new-task.gif)

### 04. AI サイドバー
> 各タスクに独立したAIセッションで会話し、ツールを実行します。

![](videos/icebox-ai-sidebar.gif)

### 05. AIでタスク作成
> 下部のAIチャットで会話しながらタスクを作成し、ボードに即反映します。

![](videos/icebox-ai-task.gif)

## 主な機能

- **5カラムカンバン** — Icebox, Emergency, In Progress, Testing, Complete
- **AI サイドバー** — タスクごとのAI会話 + ストリーミング応答
- **タスクごとのセッション** — 各タスクが独立したAI会話履歴を維持、ディスクに自動保存
- **ビルトインツール** — AIがシェル実行、ファイル読み書き、コード検索、タスク作成/更新
- **スラッシュコマンド** — ボード管理、AI制御、認証のための18コマンド
- **OAuth PKCE** — claude.aiを通じたブラウザログイン
- **マウス対応** — クリック選択、ドラッグスクロール、入力フォーカスクリック
- **テキスト選択** — ドラッグでAI応答を選択してコピー
- **スイムレーン** — タブベースのスイムレーンフィルタリング、`[`/`]`キーで切替
- **Notionスタイルリンク** — commit, PR, issue, branchの参照を自動パース
- **タスク日付** — タスクごとの開始日/期限管理
- **AIメモリ** — セッション間で維持される永続コンテキストメモリ
- **タスクストレージ** — YAMLフロントマター付きマークダウンファイル (`.icebox/tasks/`)

## インストール

### macOS

#### クイックインストール

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

#### Homebrew

```bash
brew tap SteelCrab/tap
brew install icebox
```

#### ビルド済みバイナリ (手動)

[最新リリース](https://github.com/SteelCrab/icebox/releases/latest) からダウンロード:

| アーキテクチャ | アセット |
|---|---|
| Apple Silicon (arm64) | `icebox-aarch64-apple-darwin.tar.gz` |

```bash
tar -xzf icebox-aarch64-apple-darwin.tar.gz
chmod +x icebox
mv icebox ~/.local/bin/    # または $PATH 内の任意のディレクトリ
```

### Linux

#### クイックインストール

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

#### ビルド済みバイナリ (手動)

[最新リリース](https://github.com/SteelCrab/icebox/releases/latest) からダウンロード:

| アーキテクチャ | libc | アセット |
|---|---|---|
| x86_64 | glibc | `icebox-x86_64-unknown-linux-gnu.tar.gz` |
| x86_64 | musl (Alpine) | `icebox-x86_64-unknown-linux-musl.tar.gz` |
| aarch64 | glibc | `icebox-aarch64-unknown-linux-gnu.tar.gz` |
| aarch64 | musl (Alpine) | `icebox-aarch64-unknown-linux-musl.tar.gz` |
| armv7 (Raspberry Pi 2/3) | gnueabihf | `icebox-armv7-unknown-linux-gnueabihf.tar.gz` |

```bash
tar -xzf icebox-<target>.tar.gz
chmod +x icebox
mv icebox ~/.local/bin/    # または $PATH 内の任意のディレクトリ
```

### Windows

> 💡 **WSL の使用を推奨します。** icebox の AI `bash` ツールは `grep`/`sed`/`awk`/`find` などの Unix コマンドを頻繁に呼び出しますが、Windows ネイティブでは `cmd /C` にフォールバックされ、ほとんど動作しません。WSL なら Linux バイナリがそのまま動作し、OAuth コールバックやパス処理も安定します。

#### オプション A: WSL (推奨)

**1. WSL のインストール** (Windows 10 2004+ / Windows 11)

PowerShell を**管理者として実行**:

```powershell
wsl --install
```

デフォルトの Ubuntu ディストリビューションが自動でインストールされます。再起動後にユーザー名・パスワードを設定してください。すでに WSL が入っている場合は `wsl --update` で最新化します。

**2. WSL 内で icebox をインストール**

WSL ターミナル (Ubuntu) を開き、[Linux クイックインストール](#クイックインストール) に従ってください:

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

**3. 作業ディレクトリの推奨**

パフォーマンスのため、WSL ファイルシステム (`~/projects/...`) 内で作業してください。`/mnt/c/...` (Windows ディスク) はファイル I/O が遅く、TUI のレスポンスが低下します。

**4. Windows Terminal の利用を推奨**

レガシーコンソールよりも [Windows Terminal](https://aka.ms/terminal) のほうが TUI のカラー・Unicode・マウスサポートが優れています。

#### オプション B: Windows ネイティブ (PowerShell)

WSL が利用できない環境 (社内ポリシーなど) でのみ推奨します。AI `bash` ツールの一部コマンドが動作しない場合があります。

```powershell
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$arch = if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'aarch64' } else { 'x86_64' }
$asset = "icebox-$arch-pc-windows-msvc.zip"
$dest = "$env:USERPROFILE\icebox"
Invoke-WebRequest "https://github.com/SteelCrab/icebox/releases/latest/download/$asset" -OutFile "$env:TEMP\$asset"
Expand-Archive "$env:TEMP\$asset" -DestinationPath $dest -Force
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
if (($userPath -split ';') -notcontains $dest) {
    [Environment]::SetEnvironmentVariable('Path', "$dest;$userPath", 'User')
}
$env:Path = "$dest;$env:Path"
Write-Host "icebox installed to $dest. Run 'icebox' to start."
```

> 初回起動時に SmartScreen がブロックする場合があります — **詳細情報 → 実行** をクリック (バイナリは未署名)。

##### ビルド済みバイナリ (手動)

[最新リリース](https://github.com/SteelCrab/icebox/releases/latest) からダウンロード:

| アーキテクチャ | アセット |
|---|---|
| x86_64 | `icebox-x86_64-pc-windows-msvc.zip` |
| aarch64 (ARM64) | `icebox-aarch64-pc-windows-msvc.zip` |

```powershell
Expand-Archive icebox-x86_64-pc-windows-msvc.zip -DestinationPath $env:USERPROFILE\icebox
# %USERPROFILE%\icebox を PATH に追加 (システムのプロパティ → 環境変数)
```

### ソースからビルド (全OS)

#### Cargoでインストール

```bash
cargo install --git https://github.com/SteelCrab/icebox.git
```

#### ソースからビルド

```bash
git clone https://github.com/SteelCrab/icebox.git
cd icebox
cargo build --release
cp target/release/icebox ~/.cargo/bin/
```

## クイックスタート

### 前提条件

- Rustツールチェーン (edition 2024)
- Anthropic APIキーまたはClaude.aiアカウント

### ワークスペース初期化

```bash
icebox init              # カレントディレクトリに .icebox/ を初期化
icebox init ./my-board   # 指定パスに初期化
```

### 指定パスで実行

```bash
icebox                   # カレントディレクトリでTUIを起動
icebox ./my-board        # 指定パスでTUIを起動
```

### 認証

APIキー設定（推奨）:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
icebox
```

またはOAuthログイン:

```bash
icebox login           # ブラウザが開きます
icebox login --console # コンソールベースのフロー
icebox whoami          # 認証状態を確認
```

## キーバインド

### Boardモード

| キー | 動作 |
|------|------|
| `h/l`, `←/→` | カラム移動 |
| `j/k`, `↑/↓` | タスク移動 |
| `Enter` | タスク詳細サイドバーを開く |
| `n` | 新規タスク作成 |
| `d` | タスク削除 (y/Enterで確認) |
| `>/<` | タスクを次/前のカラムへ移動 |
| `[/]` | スイムレーンタブ切替 |
| `/` | 下部AIチャットパネル切替 |
| `1/2` | タブ切替 (Board / Memory) |
| `r` | リフレッシュ |
| `q`, `Ctrl+C` | 終了 |
| マウスクリック | タスク選択 + 詳細を開く |
| マウスクリック (スイムレーンバー) | スイムレーン切替 |
| マウススクロール | ナビゲーション |

### Detailモード（サイドバー）

| キー | 動作 |
|------|------|
| `Tab`, `i` | フォーカス切替: 詳細 → チャット → 入力 |
| `e` | タスク編集 (タイトル/本文) |
| `j/k` | サイドバースクロール |
| `>/<` | タスクカラム移動 |
| `Esc` | フォーカス解除（階層的）/ ボードに戻る |
| `q` | ボードに戻る |

### Editモード

| キー | 動作 |
|------|------|
| `Tab` | タイトル ↔ 本文 切替 |
| `Ctrl+S` | 保存 |
| `Esc` | キャンセル |
| `Enter` | 次のフィールド / 改行 |

### 下部AIチャット

| キー | 動作 |
|------|------|
| `/` (Board) | パネル切替 |
| `Esc` | フォーカス解除 |
| `Enter` | メッセージ送信 |
| `Tab` | スラッシュコマンド自動補完 |
| `Ctrl+↑/↓` | パネルサイズ調整 |

## スラッシュコマンド

| カテゴリ | コマンド | 説明 |
|----------|----------|------|
| **Board** | `/new <title>` | タスク作成 |
| | `/move <column>` | タスク移動 |
| | `/delete <id>` | タスク削除 |
| | `/search <query>` | タスク検索 |
| | `/export` | ボードをマークダウンでエクスポート |
| | `/diff` | Git diff |
| | `/swimlane [name \| clear]` | スイムレーン設定/一覧 |
| **AI** | `/help` | コマンド一覧 |
| | `/status` | セッション状態 |
| | `/cost` | トークン使用量 |
| | `/clear` | 会話クリア |
| | `/compact` | 会話圧縮 |
| | `/model [name]` | モデル変更 |
| | `/remember <text>` | AIコンテキストメモリ保存 |
| | `/memory` | メモリ管理画面 |
| **Auth** | `/login` | OAuthログイン |
| | `/logout` | ログアウト |
| **Session** | `/resume [id]` | セッション復元 |

## タスクストレージ

タスクは `.icebox/tasks/{id}.md` マークダウンファイルとして保存されます:

```markdown
---
id: "uuid"
title: "タスクタイトル"
column: inprogress    # icebox | emergency | inprogress | testing | complete
priority: high        # low | medium | high | critical
tags: ["backend", "auth"]
swimlane: "backend"
start_date: "2026-04-01T00:00:00Z"
due_date: "2026-04-10T00:00:00Z"
created_at: "ISO8601"
updated_at: "ISO8601"
---

本文（マークダウン）...

## 参照
- commit:abc1234
- PR#42
- branch:feature/auth
```

AIセッションはタスクごとに `.icebox/sessions/{task_id}.json` に自動保存されます。

### ワークスペース構造

```
.icebox/
├── .gitignore          # 自動生成 (sessions/, memory.json を除外)
├── tasks/              # タスクマークダウンファイル
│   └── {id}.md
├── sessions/           # タスクごとのAIチャットセッション
│   ├── __global__.json
│   └── {task_id}.json
└── memory.json         # AIメモリエントリ
```

## アーキテクチャ

```
crates/
  icebox-cli/   # メインバイナリ — CLIサブコマンド、TUIランタイム
  tui/          # TUI — app, board, column, card, sidebar, input, layout, theme
  task/         # ドメイン — Task, Column, Priority, frontmatter, TaskStore
  api/          # API — AnthropicClient, SSEストリーミング, AuthMethod, リトライ
  runtime/      # ランタイム — ConversationRuntime, Session, OAuth PKCE, UsageTracker
  tools/        # ツール12個 — bash, read/write_file, glob/grep_search, カンバン (list/create/update/move), メモリ
  commands/     # スラッシュコマンド18個 (Board, AI, Auth, Session)
```

## ビルトインAIツール

| ツール | 説明 |
|--------|------|
| `bash` | シェルコマンド実行 |
| `read_file` | ファイル内容読み取り |
| `write_file` | ファイル作成/書き込み |
| `glob_search` | グロブパターンでファイル検索 |
| `grep_search` | 正規表現でファイル内容検索 |
| `list_tasks` | カンバンタスク一覧 |
| `create_task` | 新規タスク作成 |
| `update_task` | 既存タスク更新（タイトル、優先度、タグ、スイムレーン、日付、本文） |
| `move_task` | タスクカラム移動 |
| `save_memory` | AIコンテキストメモリ保存 |
| `list_memories` | 保存メモリ一覧 |
| `delete_memory` | メモリエントリ削除 |

## 環境変数

| 変数 | 説明 | デフォルト |
|------|------|------------|
| `ANTHROPIC_API_KEY` | APIキー（推奨） | — |
| `ANTHROPIC_AUTH_TOKEN` | Bearerトークン | — |
| `ANTHROPIC_BASE_URL` | API URL | `https://api.anthropic.com` |
| `ANTHROPIC_MODEL` | モデル | `claude-sonnet-4-20250514` |
| `ICEBOX_CONFIG_HOME` | 設定ディレクトリ | `~/.icebox` |

## ライセンス

[Apache License, Version 2.0](LICENSE)に基づき配布されます。
