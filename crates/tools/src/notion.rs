//! Notion REST API client for syncing icebox kanban tasks.
//!
//! Uses blocking reqwest to call Notion API v2022-06-28.

use anyhow::{Context, Result};
use icebox_task::model::{Column, Priority, Task};
use serde::Deserialize;
use serde_json::{json, Value};

const NOTION_API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

/// Result of a sync operation, tracking how many tasks were created,
/// updated, left unchanged, or encountered errors.
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Number of tasks newly created in Notion.
    pub created: usize,
    /// Number of existing tasks updated.
    pub updated: usize,
    /// Number of tasks that required no changes.
    pub unchanged: usize,
    /// Error messages for tasks that failed to sync.
    pub errors: Vec<String>,
}

impl std::fmt::Display for SyncResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Notion sync complete: {} created, {} updated, {} unchanged",
            self.created, self.updated, self.unchanged
        )?;
        for err in &self.errors {
            write!(f, "\n  error: {err}")?;
        }
        Ok(())
    }
}

/// A simplified Notion page reference returned from search results.
#[derive(Debug, Clone)]
pub struct NotionPage {
    /// Notion page UUID.
    pub id: String,
    /// Page title (or "(Untitled)" if absent).
    pub title: String,
}

/// Blocking Notion API client.
///
/// Wraps `reqwest::blocking::Client` with Notion-specific auth headers
/// and error handling.
pub struct NotionClient {
    http: reqwest::blocking::Client,
    api_key: String,
}

/// HTTP method selector for [`NotionClient::request`].
enum Method {
    Post,
    Patch,
}

impl NotionClient {
    /// Create a new client with the given integration API key.
    #[must_use]
    pub fn new(api_key: String) -> Self {
        Self {
            http: reqwest::blocking::Client::new(),
            api_key,
        }
    }

    /// Resolve API key from environment or config fallback.
    ///
    /// Checks `NOTION_API_KEY` env var first, then `config_api_key`.
    ///
    /// # Errors
    /// Returns an error if neither source provides a non-empty key.
    pub fn from_env(config_api_key: Option<&str>) -> Result<Self> {
        let api_key = match std::env::var("NOTION_API_KEY") {
            Ok(key) if !key.is_empty() => key,
            _ => config_api_key
                .filter(|k| !k.is_empty())
                .map(String::from)
                .context(
                    "NOTION_API_KEY 환경변수를 설정해주세요\n\
                     https://www.notion.so/my-integrations 에서 생성할 수 있습니다",
                )?,
        };
        Ok(Self::new(api_key))
    }

    /// Send an authenticated request to the Notion API and parse the
    /// JSON response. Returns a specific auth error for 401 responses.
    fn request(&self, method: Method, path: &str, body: &Value) -> Result<Value> {
        let url = format!("{NOTION_API_BASE}{path}");
        let builder = match method {
            Method::Post => self.http.post(&url),
            Method::Patch => self.http.patch(&url),
        };

        let resp = builder
            .bearer_auth(&self.api_key)
            .header("Notion-Version", NOTION_VERSION)
            .json(body)
            .send()
            .context("Notion API 요청 실패")?;

        let status = resp.status();
        let resp_body: Value = resp.json().context("Notion 응답 파싱 실패")?;

        if !status.is_success() {
            let message = resp_body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            if status.as_u16() == 401 {
                anyhow::bail!(
                    "Notion 인증 실패 (401): NOTION_API_KEY를 확인해주세요\n{message}"
                );
            }
            anyhow::bail!("Notion API 에러 ({status}): {message}");
        }
        Ok(resp_body)
    }

    /// Send a POST request to the Notion API.
    fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.request(Method::Post, path, body)
    }

    /// Send a PATCH request to the Notion API.
    fn patch(&self, path: &str, body: &Value) -> Result<Value> {
        self.request(Method::Patch, path, body)
    }

    /// Search the Notion workspace for pages matching a query string.
    ///
    /// # Errors
    /// Returns an error if the API call fails or the response cannot be parsed.
    pub fn search_pages(&self, query: &str) -> Result<Vec<NotionPage>> {
        let body = json!({
            "query": query,
            "filter": { "value": "page", "property": "object" },
            "page_size": 20
        });
        let resp = self.post("/search", &body)?;
        let results = resp
            .get("results")
            .and_then(Value::as_array)
            .context("Notion 검색 결과 파싱 실패")?;

        let pages = results
            .iter()
            .filter_map(|r| {
                let id = r.get("id")?.as_str()?.to_owned();
                let title = extract_title(r).unwrap_or_else(|| "(Untitled)".into());
                Some(NotionPage { id, title })
            })
            .collect();
        Ok(pages)
    }

    /// Create a database under a parent page with the icebox kanban schema.
    ///
    /// # Errors
    /// Returns an error if the API call fails or the response is missing a database ID.
    pub fn create_database(&self, parent_page_id: &str) -> Result<String> {
        let body = json!({
            "parent": { "type": "page_id", "page_id": parent_page_id },
            "title": [{ "type": "text", "text": { "content": "Icebox Kanban" } }],
            "properties": {
                "Name": { "title": {} },
                "Task ID": { "rich_text": {} },
                "Status": {
                    "select": {
                        "options": [
                            { "name": "Icebox", "color": "default" },
                            { "name": "Emergency", "color": "red" },
                            { "name": "In Progress", "color": "blue" },
                            { "name": "Testing", "color": "yellow" },
                            { "name": "Complete", "color": "green" }
                        ]
                    }
                },
                "Priority": {
                    "select": {
                        "options": [
                            { "name": "Low", "color": "default" },
                            { "name": "Medium", "color": "yellow" },
                            { "name": "High", "color": "orange" },
                            { "name": "Critical", "color": "red" }
                        ]
                    }
                },
                "Tags": { "multi_select": { "options": [] } },
                "Start Date": { "date": {} },
                "Due Date": { "date": {} },
                "Progress": { "rich_text": {} },
                "Created At": { "date": {} },
                "Updated At": { "date": {} }
            }
        });
        let resp = self.post("/databases", &body)?;
        resp.get("id")
            .and_then(Value::as_str)
            .map(String::from)
            .context("Notion DB 생성 응답에서 ID를 찾을 수 없습니다")
    }

    /// Query all pages in a database, handling pagination automatically.
    fn query_all_pages(&self, database_id: &str) -> Result<Vec<Value>> {
        let mut all_pages = Vec::new();
        let mut start_cursor: Option<String> = None;

        loop {
            let mut body = json!({ "page_size": 100 });
            if let Some(cursor) = &start_cursor {
                body.as_object_mut()
                    .context("failed to construct pagination body")?
                    .insert("start_cursor".into(), json!(cursor));
            }

            let resp = self.post(&format!("/databases/{database_id}/query"), &body)?;
            if let Some(results) = resp.get("results").and_then(Value::as_array) {
                all_pages.extend(results.iter().cloned());
            }

            let has_more = resp
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !has_more {
                break;
            }
            start_cursor = resp
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(String::from);
        }
        Ok(all_pages)
    }

    /// Create a task page in the Notion database.
    fn create_task_page(&self, database_id: &str, task: &Task) -> Result<String> {
        let body = json!({
            "parent": { "database_id": database_id },
            "properties": build_properties(task)?,
            "children": markdown_to_blocks(&task.body)
        });
        let resp = self.post("/pages", &body)?;
        resp.get("id")
            .and_then(Value::as_str)
            .map(String::from)
            .context("Notion 페이지 생성 응답에서 ID를 찾을 수 없습니다")
    }

    /// Update an existing task page's properties.
    fn update_task_page(&self, page_id: &str, task: &Task) -> Result<()> {
        let body = json!({ "properties": build_properties(task)? });
        self.patch(&format!("/pages/{page_id}"), &body)?;
        Ok(())
    }

    /// Sync all tasks to a Notion database, creating or updating as needed.
    ///
    /// Builds a mapping of existing `Task ID` properties to Notion page IDs,
    /// then creates new pages or updates existing ones accordingly.
    ///
    /// # Errors
    /// Returns an error if the initial database query fails. Individual task
    /// sync failures are collected in [`SyncResult::errors`].
    pub fn sync_tasks(&self, database_id: &str, tasks: &[Task]) -> Result<SyncResult> {
        let existing = self.query_all_pages(database_id)?;
        let id_map = build_page_id_map(&existing);
        let mut result = SyncResult::default();

        for task in tasks {
            match id_map.get(&task.id) {
                Some((page_id, _)) => match self.update_task_page(page_id, task) {
                    Ok(()) => result.updated += 1,
                    Err(e) => result.errors.push(format!("{}: {e}", task.id)),
                },
                None => match self.create_task_page(database_id, task) {
                    Ok(_) => result.created += 1,
                    Err(e) => result.errors.push(format!("{}: {e}", task.id)),
                },
            }
        }
        Ok(result)
    }
}

// --- Helper functions ---

/// Build a mapping from `Task ID` property values to `(page_id, last_edited_time)`.
///
/// Skips pages that have no parseable Task ID or page ID.
fn build_page_id_map(pages: &[Value]) -> std::collections::HashMap<String, (String, String)> {
    pages
        .iter()
        .filter_map(|page| {
            let task_id = extract_task_id_property(page)?;
            let page_id = page.get("id")?.as_str()?.to_owned();
            let updated_at = page
                .get("last_edited_time")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Some((task_id, (page_id, updated_at)))
        })
        .collect()
}

/// Build Notion property values from an icebox Task.
fn build_properties(task: &Task) -> Result<Value> {
    let mut props = json!({
        "Name": {
            "title": [{ "text": { "content": &task.title } }]
        },
        "Task ID": {
            "rich_text": [{ "text": { "content": &task.id } }]
        },
        "Status": {
            "select": { "name": column_to_notion(&task.column) }
        },
        "Priority": {
            "select": { "name": priority_to_notion(&task.priority) }
        }
    });

    let obj = props
        .as_object_mut()
        .context("failed to construct properties JSON object")?;

    // Tags
    if !task.tags.is_empty() {
        let tags: Vec<Value> = task.tags.iter().map(|t| json!({ "name": t })).collect();
        obj.insert("Tags".into(), json!({ "multi_select": tags }));
    }

    // Dates
    if let Some(dt) = &task.start_date {
        obj.insert(
            "Start Date".into(),
            json!({ "date": { "start": dt.to_rfc3339() } }),
        );
    }
    if let Some(dt) = &task.due_date {
        obj.insert(
            "Due Date".into(),
            json!({ "date": { "start": dt.to_rfc3339() } }),
        );
    }

    // Progress
    if let Some(prog) = &task.progress {
        obj.insert(
            "Progress".into(),
            json!({ "rich_text": [{ "text": { "content": prog.display() } }] }),
        );
    }

    // Timestamps
    obj.insert(
        "Created At".into(),
        json!({ "date": { "start": task.created_at.to_rfc3339() } }),
    );
    obj.insert(
        "Updated At".into(),
        json!({ "date": { "start": task.updated_at.to_rfc3339() } }),
    );

    Ok(props)
}

fn column_to_notion(col: &Column) -> &'static str {
    match col {
        Column::Icebox => "Icebox",
        Column::Emergency => "Emergency",
        Column::InProgress => "In Progress",
        Column::Testing => "Testing",
        Column::Complete => "Complete",
    }
}

fn priority_to_notion(pri: &Priority) -> &'static str {
    match pri {
        Priority::Low => "Low",
        Priority::Medium => "Medium",
        Priority::High => "High",
        Priority::Critical => "Critical",
    }
}

/// Extract the "Task ID" rich_text property value from a Notion page.
fn extract_task_id_property(page: &Value) -> Option<String> {
    page.get("properties")?
        .get("Task ID")?
        .get("rich_text")?
        .as_array()?
        .first()?
        .get("text")?
        .get("content")?
        .as_str()
        .map(String::from)
}

/// Extract page title from a Notion search result.
///
/// Tries common property key names (`title`, `Name`, `name`) and
/// collects all `plain_text` segments into a single string.
fn extract_title(page: &Value) -> Option<String> {
    let props = page.get("properties")?;
    for key in ["title", "Name", "name"] {
        let arr = props.get(key)?.get("title")?.as_array()?;
        let text: String = arr
            .iter()
            .filter_map(|t| t.get("plain_text").and_then(Value::as_str))
            .collect();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Convert a simple markdown body to Notion block objects.
///
/// Recognizes `#`/`##`/`###` headings, `- ` bulleted list items,
/// and plain paragraphs. Blank lines are skipped.
fn markdown_to_blocks(body: &str) -> Vec<Value> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(line_to_block)
        .collect()
}

/// Convert a single non-empty line into the corresponding Notion block.
fn line_to_block(line: &str) -> Value {
    // Check headings in descending specificity (### before ## before #)
    // to avoid `## Foo` matching the `#` branch.
    if let Some(h) = line.strip_prefix("### ") {
        rich_text_block("heading_3", h.trim())
    } else if let Some(h) = line.strip_prefix("## ") {
        rich_text_block("heading_2", h.trim())
    } else if let Some(h) = line.strip_prefix("# ") {
        rich_text_block("heading_1", h.trim())
    } else if let Some(item) = line.strip_prefix("- ") {
        rich_text_block("bulleted_list_item", item.trim())
    } else {
        rich_text_block("paragraph", line)
    }
}

/// Build a Notion block object with a single rich-text segment.
fn rich_text_block(block_type: &str, content: &str) -> Value {
    json!({
        "object": "block",
        "type": block_type,
        block_type: {
            "rich_text": [{ "type": "text", "text": { "content": content } }]
        }
    })
}

/// Input schema for the `notion_sync` AI tool.
#[derive(Deserialize)]
pub struct NotionSyncInput {
    /// Action to perform: `"push"` or `"status"`.
    pub action: String,
    /// Parent page name for initial database setup (only needed once).
    pub page_name: Option<String>,
}
