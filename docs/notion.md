# Notion Sync Guide

> Sync your icebox kanban tasks with a Notion database.
> Free — Notion API has no per-call charges.

## What You Can Do

- Push kanban tasks to a Notion database with one command
- Pull edits made in Notion back into icebox
- Collaborate via Notion board, table, or calendar views
- No additional cost — Notion API is free on every workspace plan

## Prerequisites

### 01. Create a Notion Integration

1. Visit [https://www.notion.so/my-integrations](https://www.notion.so/my-integrations)
2. Click **New integration**, give it a name (e.g. `icebox`), and click **Submit**
3. Copy the **Internal Integration Secret** — it starts with `ntn_`

![Create Notion Integration](../images/notion-integration-create.png)

### 02. Set the API key as an environment variable

```bash
export NOTION_API_KEY="ntn_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

For convenience, persist this in your shell profile (`~/.zshrc`, `~/.bashrc`, etc.) and reload your shell.

> icebox reads `NOTION_API_KEY` from the environment first. If unset, it falls back to `notion.api_key` in `~/.icebox/config.json` (file mode `0o600`).

### 03. Share a parent page with the Integration

icebox creates the task database **under a page you choose**, so the Integration must have access to that page.

1. Open the page in Notion that will host your task database
2. Click the `⋯` menu in the top-right
3. Choose **Add connections** → select your Integration

![Share page with Integration](../images/notion-share-page.png)

> Without this step, the page will not appear in the icebox picker and database creation will fail.

## Quick Start

### Push tasks to Notion

Inside icebox, run:

```
/notion push
```

**First run.** A list of pages your Integration can access is shown in the sidebar. Type the number of the parent page to confirm — icebox creates a new database under it and uploads every task in your board.

**Subsequent runs.** icebox reuses the saved database and performs an incremental sync:

- New tasks → created
- Modified tasks → updated (matched by `Task ID`)
- Unchanged tasks → skipped

A summary line reports the counts when the sync finishes.

![/notion push demo](../videos/icebox-notion-push.gif)

### View your tasks in Notion

Open the database in Notion. You can switch between board, table, calendar, or timeline views and filter by `Status`, `Priority`, `Tags`, and more.

![Notion database result](../videos/icebox-notion-result.gif)

## Slash Commands

| Command | Description |
|---------|-------------|
| `/notion push`   | Sync icebox → Notion (incremental after the first run) |
| `/notion pull`   | Sync Notion → icebox (apply remote edits locally) |
| `/notion status` | Show connection state and last sync info |
| `/notion reset`  | Clear the saved database settings (run before switching databases) |

## Field Mapping

| Icebox field | Notion property | Type |
|--------------|-----------------|------|
| `title`      | Name        | title |
| `id`         | Task ID     | rich_text (matching key) |
| `column`     | Status      | select — `Icebox` / `Emergency` / `In Progress` / `Testing` / `Complete` |
| `priority`   | Priority    | select — `Low` / `Medium` / `High` / `Critical` |
| `tags`       | Tags        | multi_select |
| `start_date` | Start Date  | date |
| `due_date`   | Due Date    | date |
| `progress`   | Progress    | rich_text (`done/total`) |
| `created_at` | Created At  | date |
| `updated_at` | Updated At  | date |
| `body`       | Page content | blocks (markdown converted to Notion blocks) |

`Task ID` is the matching key between an icebox task and a Notion page. Do not edit it manually in Notion.

## FAQ

**Does it cost anything?**
No. Notion API is free on every workspace plan (Free, Plus, Business, Enterprise). Pricing is by workspace tier, not by API usage.

**Where is the API key stored?**
icebox reads `NOTION_API_KEY` from the environment first. If unset, it falls back to `notion.api_key` in `~/.icebox/config.json` (Unix file mode `0o600`, readable only by you).

**What happens to tasks I delete in Notion?**
`/notion pull` updates icebox to match the current state of the Notion database. If you want a hard delete on both sides, remove the task in each system separately.

**Can I sync multiple workspaces?**
icebox saves one database per workspace today. To switch, run `/notion reset` and then `/notion push` again to pick a new parent page.

**How are conflicts resolved?**
Last write wins, decided by `updated_at`. The side with the newer timestamp overwrites the other during sync.

**Do I need to share every task page with the Integration?**
No. Sharing the **parent page** is enough — every database and child page beneath it inherits access automatically.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `NOTION_API_KEY is not set` | Export the env var and restart icebox |
| `401 Unauthorized` | Verify the key — it must start with `ntn_` and belong to your workspace |
| `Could not find page` | Make sure the Integration is connected to the parent page |
| `429 Rate limited` | icebox retries automatically; try again in a moment |
| Page not in picker list | Add the Integration to that page via `⋯` → Add connections |

## References

- [Notion Integrations dashboard](https://www.notion.so/my-integrations)
- [Notion API documentation](https://developers.notion.com/)
- [icebox repository](https://github.com/SteelCrab/icebox)
