# Icebox

**Rust TUI 칸반보드 + AI 사이드바**

[English](README.md) | [한국어](README.ko.md)

Rust로 만든 터미널 기반 칸반보드로, Anthropic API를 활용한 AI 어시스턴트가 통합되어 있습니다. Vim 스타일 키바인딩으로 태스크를 관리하고, 태스크별 AI 채팅으로 대화하며, AI가 보드와 파일시스템을 직접 조작할 수 있습니다.

## 주요 기능

- **5컬럼 칸반보드** — Icebox, Emergency, In Progress, Testing, Complete
- **AI 사이드바** — 태스크별 AI 대화 + 스트리밍 응답
- **태스크별 세션** — 각 태스크가 독립 AI 대화 히스토리를 유지, 디스크에 자동 저장
- **빌트인 도구** — AI가 셸 명령 실행, 파일 읽기/쓰기, 코드 검색, 태스크 생성/수정 가능
- **슬래시 명령어** — 보드 관리, AI 제어, 인증을 위한 17개 명령어
- **OAuth PKCE** — claude.ai를 통한 브라우저 로그인
- **마우스 지원** — 클릭 선택, 드래그 스크롤, 입력 포커스 클릭
- **텍스트 선택** — 드래그로 AI 응답 선택 및 복사
- **Notion 스타일 링크** — commit, PR, issue, branch 참조 자동 파싱
- **태스크 날짜** — 태스크별 시작일/종료일 관리
- **AI 메모리** — 세션 간 유지되는 AI 컨텍스트 메모리
- **태스크 저장소** — YAML frontmatter 포함 마크다운 파일 (`.icebox/tasks/`)

## 설치

### 빠른 설치

```bash
curl -fsSL https://raw.githubusercontent.com/SteelCrab/icebox/main/install.sh | bash
```

### Homebrew

```bash
brew tap SteelCrab/tap
brew install icebox
```

### 소스 설치 (cargo install)

```bash
cargo install --git https://github.com/SteelCrab/icebox.git
```

### 소스 빌드

```bash
git clone https://github.com/SteelCrab/icebox.git
cd icebox
cargo build --release
cp target/release/icebox ~/.cargo/bin/
```

## 빠른 시작

### 사전 요구사항

- Rust 툴체인 (edition 2024)
- Anthropic API 키 또는 Claude.ai 계정

### 워크스페이스 초기화

```bash
icebox init              # 현재 디렉토리에 .icebox/ 초기화
icebox init ./my-board   # 지정 경로에 초기화
```

### 특정 경로에서 실행

```bash
icebox                   # 현재 디렉토리에서 TUI 실행
icebox ./my-board        # 지정 경로에서 TUI 실행
```

### 인증

API 키 설정 (권장):

```bash
export ANTHROPIC_API_KEY=sk-ant-...
icebox
```

또는 OAuth 로그인:

```bash
icebox login           # 브라우저 열림
icebox login --console # 콘솔 기반 플로우
icebox whoami          # 인증 상태 확인
```

## 키바인딩

### Board 모드

| 키 | 동작 |
|----|------|
| `h/l`, `←/→` | 컬럼 이동 |
| `j/k`, `↑/↓` | 태스크 이동 |
| `Enter` | 태스크 상세 사이드바 열기 |
| `n` | 새 태스크 생성 |
| `d` | 태스크 삭제 (y/Enter 확인) |
| `>/<` | 태스크를 다음/이전 컬럼으로 이동 |
| `/` | 하단 AI 채팅 패널 토글 |
| `1/2` | 탭 전환 (Board / Memory) |
| `r` | 새로고침 |
| `q`, `Ctrl+C` | 종료 |
| 마우스 클릭 | 태스크 선택 + 상세 열기 |
| 마우스 스크롤 | 네비게이션 |

### Detail 모드 (사이드바)

| 키 | 동작 |
|----|------|
| `Tab`, `i` | 포커스 순환: 상세 → 채팅 → 입력 |
| `e` | 태스크 편집 모드 (Title/Body 수정) |
| `j/k` | 사이드바 스크롤 |
| `>/<` | 태스크 컬럼 이동 |
| `Esc` | 포커스 해제 (계층적) / 보드로 복귀 |
| `q` | 보드로 복귀 |

### Edit 모드

| 키 | 동작 |
|----|------|
| `Tab` | Title ↔ Body 전환 |
| `Ctrl+S` | 저장 |
| `Esc` | 취소 |
| `Enter` | 다음 필드 이동 / 줄바꿈 |

### 하단 AI 채팅

| 키 | 동작 |
|----|------|
| `/` (Board) | 패널 토글 |
| `Esc` | 포커스 해제 |
| `Enter` | 메시지 전송 |
| `Tab` | 슬래시 명령어 자동완성 |
| `Ctrl+↑/↓` | 패널 크기 조절 |

## 슬래시 명령어

| 카테고리 | 명령 | 설명 |
|----------|------|------|
| **Board** | `/new <title>` | 태스크 생성 |
| | `/move <column>` | 태스크 이동 |
| | `/delete <id>` | 태스크 삭제 |
| | `/search <query>` | 태스크 검색 |
| | `/export` | 보드 마크다운 내보내기 |
| | `/diff` | Git diff |
| **AI** | `/help` | 명령어 목록 |
| | `/status` | 세션 상태 |
| | `/cost` | 토큰 비용 |
| | `/clear` | 대화 초기화 |
| | `/compact` | 대화 압축 |
| | `/model [name]` | 모델 변경 |
| | `/remember <text>` | AI 컨텍스트 메모리 저장 |
| | `/memory` | 메모리 관리 화면 |
| **Auth** | `/login` | OAuth 로그인 |
| | `/logout` | 로그아웃 |
| **Session** | `/resume [id]` | 세션 복원 |

## 태스크 저장

태스크는 `.icebox/tasks/{id}.md` 마크다운 파일로 저장됩니다:

```markdown
---
id: "uuid"
title: "태스크 제목"
column: inprogress    # icebox | emergency | inprogress | testing | complete
priority: high        # low | medium | high | critical
tags: ["backend", "auth"]
start_date: "2026-04-01T00:00:00Z"
due_date: "2026-04-10T00:00:00Z"
created_at: "ISO8601"
updated_at: "ISO8601"
---

본문 (마크다운)...

## 참조
- commit:abc1234
- PR#42
- branch:feature/auth
```

AI 세션은 태스크별로 `.icebox/sessions/{task_id}.json`에 자동 저장됩니다.

### 워크스페이스 구조

```
.icebox/
├── .gitignore          # 자동 생성 (sessions/, memory.json 제외)
├── tasks/              # 태스크 마크다운 파일
│   └── {id}.md
├── sessions/           # 태스크별 AI 채팅 세션
│   ├── __global__.json
│   └── {task_id}.json
└── memory.json         # AI 메모리 엔트리
```

## 아키텍처

```
crates/
  icebox-cli/   # 메인 바이너리 — CLI 서브커맨드, TUI 런타임
  tui/          # TUI — app, board, column, card, sidebar, input, layout, theme
  task/         # 도메인 — Task, Column, Priority, frontmatter, TaskStore
  api/          # API — AnthropicClient, SSE 스트리밍, AuthMethod, 재시도
  runtime/      # 런타임 — ConversationRuntime, Session, OAuth PKCE, UsageTracker
  tools/        # 도구 12개 — bash, read/write_file, glob/grep_search, 칸반 (list/create/update/move), 메모리
  commands/     # 슬래시 명령어 17개 (Board, AI, Auth, Session)
```

## 빌트인 AI 도구

| 도구 | 설명 |
|------|------|
| `bash` | 셸 명령 실행 |
| `read_file` | 파일 내용 읽기 |
| `write_file` | 파일 생성/쓰기 |
| `glob_search` | 글로브 패턴으로 파일 검색 |
| `grep_search` | 정규식으로 파일 내용 검색 |
| `list_tasks` | 칸반보드 태스크 목록 조회 |
| `create_task` | 새 태스크 생성 |
| `update_task` | 기존 태스크 수정 (제목, 우선순위, 태그, 날짜, 본문) |
| `move_task` | 태스크 컬럼 이동 |
| `save_memory` | AI 컨텍스트 메모리 저장 |
| `list_memories` | 저장된 메모리 목록 |
| `delete_memory` | 메모리 항목 삭제 |

## 환경 변수

| 변수 | 설명 | 기본값 |
|------|------|--------|
| `ANTHROPIC_API_KEY` | API 키 (권장) | — |
| `ANTHROPIC_AUTH_TOKEN` | Bearer 토큰 | — |
| `ANTHROPIC_BASE_URL` | API URL | `https://api.anthropic.com` |
| `ANTHROPIC_MODEL` | 모델 | `claude-sonnet-4-20250514` |
| `ICEBOX_CONFIG_HOME` | 설정 디렉토리 | `~/.icebox` |

## 라이선스

[Apache License, Version 2.0](LICENSE)에 따라 배포됩니다.
