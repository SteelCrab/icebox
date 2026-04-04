use serde::{Deserialize, Serialize};

/// A reference link that can appear in task markdown.
/// Rendered as a Notion-style block in the TUI sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLink {
    pub kind: LinkKind,
    pub label: String,
    pub url: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Commit,
    PR,
    Issue,
    Branch,
    Url,
}

impl LinkKind {
    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Commit => "\u{25cb}", // ○
            Self::PR => "\u{21b3}",     // ↳
            Self::Issue => "\u{25cf}",  // ●
            Self::Branch => "\u{2387}", // ⎇
            Self::Url => "\u{1f517}",   // 🔗  (but we'll use a simpler char)
        }
    }

    #[must_use]
    pub fn label_prefix(&self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::PR => "PR",
            Self::Issue => "issue",
            Self::Branch => "branch",
            Self::Url => "link",
        }
    }
}

/// Configuration for resolving short references to full URLs.
#[derive(Debug, Clone, Default)]
pub struct LinkResolver {
    /// GitHub repository in "owner/repo" format
    pub github_repo: Option<String>,
}

impl LinkResolver {
    #[must_use]
    pub fn resolve_url(&self, link: &TaskLink) -> Option<String> {
        if link.url.is_some() {
            return link.url.clone();
        }

        let repo = self.github_repo.as_deref()?;
        let base = format!("https://github.com/{repo}");

        match link.kind {
            LinkKind::Commit => Some(format!("{base}/commit/{}", link.label)),
            LinkKind::PR => {
                let num = link.label.trim_start_matches('#');
                Some(format!("{base}/pull/{num}"))
            }
            LinkKind::Issue => {
                let num = link.label.trim_start_matches('#');
                Some(format!("{base}/issues/{num}"))
            }
            LinkKind::Branch => Some(format!("{base}/tree/{}", link.label)),
            LinkKind::Url => None,
        }
    }
}

/// Parse task body text and extract all link references.
///
/// Supported formats:
///   `commit:abc1234` or `commit:abc1234def`
///   `PR#123` or `pr#123` or `PR #123`
///   `issue#45` or `issue #45` or `#45` (when preceded by issue context)
///   `branch:feature/foo` or `branch:main`
///   `https://...` or `http://...` (full URLs)
///   `owner/repo#123` (GitHub shorthand)
pub fn parse_links(text: &str) -> Vec<TaskLink> {
    let mut links = Vec::new();

    for line in text.lines() {
        parse_line_links(line, &mut links);
    }

    links
}

fn parse_line_links(line: &str, links: &mut Vec<TaskLink>) {
    let trimmed = line.trim();

    // Full URL: https://... or http://...
    for word in trimmed.split_whitespace() {
        if word.starts_with("https://") || word.starts_with("http://") {
            let url = word.trim_end_matches(['.', ',', ')']);
            let kind = classify_url(url);
            let label = shorten_url(url);
            links.push(TaskLink {
                kind,
                label,
                url: Some(url.to_string()),
                raw: url.to_string(),
            });
        }
    }

    // commit:<sha>
    for cap in find_pattern(trimmed, "commit:") {
        let sha = cap.trim();
        if !sha.is_empty() && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            links.push(TaskLink {
                kind: LinkKind::Commit,
                label: truncate_sha(sha),
                url: None,
                raw: format!("commit:{sha}"),
            });
        }
    }

    // branch:<name>
    for cap in find_pattern(trimmed, "branch:") {
        let name = cap.trim();
        if !name.is_empty() {
            links.push(TaskLink {
                kind: LinkKind::Branch,
                label: name.to_string(),
                url: None,
                raw: format!("branch:{name}"),
            });
        }
    }

    // PR#123, pr#123, PR #123
    parse_numbered_ref(
        trimmed,
        &["PR#", "pr#", "PR #", "pr #"],
        LinkKind::PR,
        links,
    );

    // issue#123, issue #123
    parse_numbered_ref(
        trimmed,
        &["issue#", "issue #", "Issue#", "Issue #"],
        LinkKind::Issue,
        links,
    );

    // owner/repo#123 (GitHub shorthand)
    for word in trimmed.split_whitespace() {
        if let Some((repo_part, num_part)) = word.split_once('#')
            && repo_part.contains('/')
            && !num_part.is_empty()
            && num_part.chars().all(|c| c.is_ascii_digit())
        {
            links.push(TaskLink {
                kind: LinkKind::Issue,
                label: word.to_string(),
                url: Some(format!("https://github.com/{repo_part}/issues/{num_part}")),
                raw: word.to_string(),
            });
        }
    }
}

fn find_pattern<'a>(text: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut results = Vec::new();
    let mut search = text;

    while let Some(pos) = search.find(prefix) {
        let after = &search[pos + prefix.len()..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == ',' || c == ')' || c == ']')
            .unwrap_or(after.len());
        let value = &after[..end];
        if !value.is_empty() {
            results.push(value);
        }
        search = &after[end..];
    }

    results
}

fn parse_numbered_ref(text: &str, prefixes: &[&str], kind: LinkKind, links: &mut Vec<TaskLink>) {
    for prefix in prefixes {
        for word_start in find_pattern(text, prefix) {
            let num: String = word_start
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !num.is_empty() {
                let label = format!("#{num}");
                // Avoid duplicates
                if !links.iter().any(|l| l.kind == kind && l.label == label) {
                    links.push(TaskLink {
                        kind,
                        label,
                        url: None,
                        raw: format!("{}{num}", kind.label_prefix()),
                    });
                }
            }
        }
    }
}

fn classify_url(url: &str) -> LinkKind {
    if url.contains("/commit/") || url.contains("/commits/") {
        LinkKind::Commit
    } else if url.contains("/pull/") || url.contains("/merge_requests/") {
        LinkKind::PR
    } else if url.contains("/issues/") {
        LinkKind::Issue
    } else if url.contains("/tree/") || url.contains("/branch/") {
        LinkKind::Branch
    } else {
        LinkKind::Url
    }
}

fn shorten_url(url: &str) -> String {
    // Extract meaningful part from GitHub URLs
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = rest.splitn(4, '/').collect();
        match parts.as_slice() {
            [owner, repo, "pull", num] => return format!("{owner}/{repo}#{num}"),
            [owner, repo, "issues", num] => return format!("{owner}/{repo}#{num}"),
            [owner, repo, "commit", sha] => {
                return format!("{owner}/{repo}@{}", truncate_sha(sha));
            }
            [owner, repo, "tree", branch] => {
                return format!("{owner}/{repo}:{branch}");
            }
            _ => {}
        }
    }

    // Generic shortening
    let without_proto = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let short: String = without_proto.chars().take(50).collect();
    if without_proto.len() > 50 {
        format!("{short}...")
    } else {
        short
    }
}

fn truncate_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// Format an OSC 8 hyperlink escape sequence for terminals.
/// `\x1b]8;;URL\x1b\\LABEL\x1b]8;;\x1b\\`
#[must_use]
pub fn osc8_hyperlink(url: &str, label: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{label}\x1b]8;;\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_commit_link() {
        let text = "Fixed in commit:abc1234def";
        let links = parse_links(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::Commit);
        assert_eq!(links[0].label, "abc1234");
    }

    #[test]
    fn parse_pr_and_issue() {
        let text = "See PR#42 and issue#7 for details";
        let links = parse_links(text);
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::PR && l.label == "#42")
        );
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::Issue && l.label == "#7")
        );
    }

    #[test]
    fn parse_branch_link() {
        let text = "Working on branch:feature/auth-flow";
        let links = parse_links(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::Branch);
        assert_eq!(links[0].label, "feature/auth-flow");
    }

    #[test]
    fn parse_github_url() {
        let text = "https://github.com/owner/repo/pull/123";
        let links = parse_links(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::PR);
        assert_eq!(links[0].label, "owner/repo#123");
    }

    #[test]
    fn parse_github_shorthand() {
        let text = "Related: anthropics/claude-code#100";
        let links = parse_links(text);
        assert!(
            links
                .iter()
                .any(|l| l.kind == LinkKind::Issue && l.label == "anthropics/claude-code#100")
        );
    }

    #[test]
    fn resolve_with_github_repo() {
        let resolver = LinkResolver {
            github_repo: Some("pista/icebox".to_string()),
        };
        let link = TaskLink {
            kind: LinkKind::PR,
            label: "#42".to_string(),
            url: None,
            raw: "PR#42".to_string(),
        };
        let url = resolver.resolve_url(&link);
        assert_eq!(
            url,
            Some("https://github.com/pista/icebox/pull/42".to_string())
        );
    }

    #[test]
    fn empty_text_returns_no_links() {
        let links = parse_links("");
        assert!(links.is_empty());
    }
}
