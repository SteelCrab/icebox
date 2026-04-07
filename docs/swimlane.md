# Swimlane Guide

> Group related tasks under a label and filter the board to one group at a time.
> Each swimlane is just a string on the task — no schema, no setup.

## What You Can Do

- Tag any task with a swimlane label (e.g., `backend`, `frontend`, `q1-2026`)
- Filter the board to a single swimlane via the tab bar at the top
- Cycle through swimlanes with `]` / `[`
- Set or clear the swimlane via slash command, key, or the create form
- New tasks created from a filtered view inherit that swimlane automatically

## How It Works

A swimlane is a free-form string stored on the task itself. There is no separate
registry — the tab bar auto-collects unique swimlane names from your current tasks.

- The "All" tab shows every task.
- Selecting a tab filters the board to tasks whose `swimlane` matches.
- Tasks without a swimlane only appear under "All".
- A tab disappears the moment its last task is renamed or cleared.

![Swimlane bar](../images/icebox-swimlane-bar.png)

## Quick Start

### 01. Set a swimlane on a task

Select a task on the board, then run:

```text
/swimlane backend
```

A `backend` tab now appears in the swimlane bar at the top of the board.

### 02. Filter the board

Click the `backend` tab, or press `]` / `[` to cycle through tabs.
The board now shows only tasks tagged `backend`.

### 03. Add more tasks under the same swimlane

While the `backend` filter is active, press `n` to create a new task —
the **Swimlane** field is pre-filled with `backend` so the task lands in
the same lane automatically.

### 04. Clear it

Open the task detail view (`Enter`) and press `s`, or run:

```text
/swimlane clear
```

![/swimlane demo](../videos/icebox-swimlane.gif)

## Slash Commands

| Command | Description |
|---------|-------------|
| `/swimlane`              | List all swimlanes with task counts |
| `/swimlane <name>`       | Set swimlane on the selected task |
| `/swimlane clear`        | Remove swimlane from the selected task (also `/swimlane none`) |
| `/sl`, `/lane`           | Aliases for `/swimlane` |

## Keybindings

| Mode | Key | Action |
|------|-----|--------|
| Board  | `]`             | Next swimlane (cycles through `All`) |
| Board  | `[`             | Previous swimlane |
| Board  | mouse click     | Click any tab on the swimlane bar |
| Board  | `n`             | Open the create form (Swimlane is one of the fields) |
| Detail | `s`             | Clear the swimlane on the selected task |

In the create form, `Tab` walks the field order
`Title → Tags → Swimlane → Start Date → Due Date`.

## Field Format

Swimlane is stored in the YAML frontmatter of each task file at
`.icebox/tasks/{uuid}.md`:

```yaml
---
id: "..."
title: "Refactor auth middleware"
column: inprogress
priority: high
swimlane: backend
created_at: "..."
updated_at: "..."
---
```

- Type: optional string. Omit the field, or set it to an empty value, to leave the task uncategorized.
- You can edit the file by hand — the next reload (`r`) picks up the change.

## Tips

- Pick a small set of stable names per project (`backend`, `frontend`, `infra`)
  rather than one-off labels — the tab bar gets noisy otherwise.
- Time buckets work too: `q1-2026`, `q2-2026`, `next`.
- Swimlane is orthogonal to columns, priority, and tags — combine freely.
- Need multi-category? Use tags. Each task has exactly one swimlane.

## FAQ

- **Where does the swimlane list come from?**
  It is auto-collected from existing tasks every time the board reloads.
  Set the same name on a task to bring a removed swimlane back.

- **Can a task belong to multiple swimlanes?**
  No — one swimlane per task. Use tags if you need multiple categories.

- **What happens when I rename a swimlane?**
  Edit each task that uses the old name (or run `/swimlane <new>` on each).
  The old tab disappears on the next reload once no task references it.

- **Does swimlane filtering affect the AI sidebar?**
  No. Per-task AI sessions are tied to the task ID and persist independently
  of which swimlane is active.

- **Why does the swimlane label disappear from cards while filtered?**
  When a single swimlane is active, every visible card belongs to it, so the
  label is redundant. The label reappears under the `All` tab.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| No swimlane bar visible at the top of the board | No tasks have a swimlane yet. Run `/swimlane <name>` on a task |
| `/swimlane <name>` reports "No task selected"    | Move the cursor onto a task in the board first |
| Tab vanished after I cleared a task              | Expected — the bar only lists swimlanes that still have at least one task |
| New task did not land in the active swimlane     | Confirm the create form's Swimlane field is filled before pressing Enter |

## References

- icebox repository: https://github.com/SteelCrab/icebox
- Related: [Notion Sync Guide](./notion.md)
