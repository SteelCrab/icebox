# icebox-task

Domain models, task storage, frontmatter parsing, link detection, and memory store.

## Modules

| File | Description |
|------|-------------|
| `model.rs` | `Task`, `Column`, `Priority` — core domain types |
| `store.rs` | `TaskStore` — CRUD operations on `.icebox/tasks/*.md` |
| `frontmatter.rs` | YAML frontmatter parser/serializer |
| `links.rs` | Notion-style link parser (commit, PR, issue, branch, URL) |
| `memory.rs` | `MemoryStore` — persistent AI memory (JSON) |
| `filter.rs` | Task filtering utilities |

## Task File Format

```markdown
---
id: "uuid"
title: "Task title"
column: inprogress
priority: high
tags: ["backend"]
start_date: "2026-04-01T00:00:00Z"
due_date: "2026-04-10T00:00:00Z"
---

Body text in markdown...
```

## Storage

- Tasks: `.icebox/tasks/{uuid}.md`
- Memory: `.icebox/memory.json`
- Atomic writes (temp file + rename)
