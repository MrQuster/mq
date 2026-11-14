#![deny(clippy::await_holding_lock)]
//! Pure state models for the catalog flows built on top of the search screen:
//! project page, install dialog (dependencies / modpack confirmation), the downloads
//! queue, the manual-download dialog, plus a safe (script-free) Markdown/HTML renderer model.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mq_core::{InstanceId, ModLoaderKind};
use mq_mods::{
    BlockedDownload, CatalogHit, DependencyResolutionPlan, ProjectDetails, ProjectKind,
    ProjectVersion,
};

// ---------------------------------------------------------------------------------------------
// Safe Markdown / HTML model
// ---------------------------------------------------------------------------------------------

/// A renderable block of project description. Contains plain text only: no HTML, no scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MdBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
        links: Vec<(String, String)>,
    },
    ListItem {
        text: String,
    },
    Quote {
        text: String,
    },
    Code {
        text: String,
    },
    Rule,
    Image {
        alt: String,
        url: String,
    },
}

/// `true` for absolute `http(s)` URLs without whitespace (the only links/images we ever show).
pub fn is_safe_http_url(url: &str) -> bool {
    let u = url.trim();
    (u.starts_with("https://") || u.starts_with("http://"))
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
        && u.len() < 2048
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn find_ci(hay: &str, needle: &str, from: usize) -> Option<usize> {
    hay.get(from..)
        .and_then(|h| h.to_ascii_lowercase().find(needle))
        .map(|i| i + from)
}

/// Extracts the value of an HTML attribute (`src`, `href`, `alt`) from a raw tag body.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let start = from + i;
        let before_ok = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
        let rest = tag[start + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let rest = rest[1..].trim_start();
            let quote = rest.chars().next()?;
            if quote == '"' || quote == '\'' {
                let end = rest[1..].find(quote)?;
                return Some(rest[1..1 + end].to_string());
            }
            let end = rest
                .find(|c: char| c.is_whitespace() || c == '>')
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
        from = start + name.len();
    }
    None
}

/// Converts untrusted HTML to plain text with line structure.
/// `<script>`, `<style>`, `<iframe>`, `<object>`, `<embed>` blocks and comments are dropped with
/// their content; `<img>` becomes an `![alt](url)` marker (http/https only); anchors keep text
/// and become `[text](url)` (http/https only); all other tags are stripped.
pub fn sanitize_html(src: &str) -> String {
    const DROP: [&str; 5] = ["script", "style", "iframe", "object", "embed"];
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut pending_href: Option<String> = None;
    while i < src.len() {
        let Some(lt) = src[i..].find('<').map(|p| p + i) else {
            out.push_str(&src[i..]);
            break;
        };
        out.push_str(&src[i..lt]);
        if src[lt..].starts_with("<!--") {
            i = src[lt..].find("-->").map_or(src.len(), |p| lt + p + 3);
            continue;
        }
        let Some(gt) = src[lt..].find('>').map(|p| p + lt) else {
            // Unterminated tag: drop the rest instead of leaking raw markup.
            break;
        };
        let body = &src[lt + 1..gt];
        let closing = body.starts_with('/');
        let name: String = body
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        i = gt + 1;
        if !closing && DROP.contains(&name.as_str()) {
            let close = format!("</{name}");
            i = match find_ci(src, &close, i) {
                Some(p) => src[p..].find('>').map_or(src.len(), |q| p + q + 1),
                None => src.len(),
            };
            continue;
        }
        match name.as_str() {
            "br" | "p" | "div" | "tr" | "ul" | "ol" | "table" | "blockquote" => out.push('\n'),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                out.push('\n');
                if !closing {
                    let level: usize = name[1..].parse().unwrap_or(1);
                    out.push_str(&"#".repeat(level));
                    out.push(' ');
                }
            }
            "li" => {
                out.push('\n');
                if !closing {
                    out.push_str("- ");
                }
            }
            "hr" => out.push_str("\n---\n"),
            "img" => {
                if let Some(url) = attr_value(body, "src").filter(|u| is_safe_http_url(u)) {
                    let alt = attr_value(body, "alt").unwrap_or_default();
                    out.push_str(&format!("\n![{}]({})\n", alt.replace(['[', ']'], ""), url));
                }
            }
            "a" => {
                if closing {
                    if let Some(h) = pending_href.take() {
                        out.push_str(&format!("]({h})"));
                    }
                } else if let Some(h) = attr_value(body, "href").filter(|u| is_safe_http_url(u)) {
                    out.push('[');
                    pending_href = Some(h);
                }
            }
            _ => {}
        }
    }
    decode_entities(&out)
}

/// Splits `[text](url)` links out of one line; returns plain text plus the safe links found.
fn extract_inline(line: &str) -> (String, Vec<(String, String)>) {
    let mut text = String::new();
    let mut links = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(close) = chars[i..].iter().position(|c| *c == ']').map(|p| p + i) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = chars[close..]
                        .iter()
                        .position(|c| *c == ')')
                        .map(|p| p + close)
                    {
                        let label: String = chars[i + 1..close].iter().collect();
                        let url: String = chars[close + 2..end].iter().collect();
                        text.push_str(&label);
                        if is_safe_http_url(&url) {
                            links.push((label, url));
                        }
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if matches!(chars[i], '*' | '_' | '`') && chars.get(i + 1) == Some(&chars[i]) {
            i += 2;
            continue;
        }
        if chars[i] == '`' {
            i += 1;
            continue;
        }
        text.push(chars[i]);
        i += 1;
    }
    (text.trim().to_string(), links)
}

/// Parses an untrusted Markdown/HTML project description into safe blocks.
pub fn parse_description(src: &str) -> Vec<MdBlock> {
    let clean = sanitize_html(src);
    let mut blocks = Vec::new();
    let mut para = String::new();
    let mut para_links: Vec<(String, String)> = Vec::new();
    let mut in_code = false;
    let mut code = String::new();

    macro_rules! flush {
        () => {
            if !para.trim().is_empty() {
                blocks.push(MdBlock::Paragraph {
                    text: para.trim().to_string(),
                    links: std::mem::take(&mut para_links),
                });
            }
            para.clear();
            para_links.clear();
        };
    }

    for raw in clean.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") {
            if in_code {
                blocks.push(MdBlock::Code {
                    text: code.trim_end().to_string(),
                });
                code.clear();
            } else {
                flush!();
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            code.push_str(line);
            code.push('\n');
            continue;
        }
        let t = line.trim();
        if t.is_empty() {
            flush!();
        } else if t == "---" || t == "***" || t == "___" {
            flush!();
            blocks.push(MdBlock::Rule);
        } else if let Some(rest) = t.strip_prefix("![") {
            flush!();
            if let Some(mid) = rest.find("](") {
                if let Some(end) = rest[mid..].find(')') {
                    let url = &rest[mid + 2..mid + end];
                    if is_safe_http_url(url) {
                        blocks.push(MdBlock::Image {
                            alt: rest[..mid].to_string(),
                            url: url.to_string(),
                        });
                    }
                }
            }
        } else if t.starts_with('#') {
            flush!();
            let level = t.chars().take_while(|c| *c == '#').count().min(6) as u8;
            let (text, _) = extract_inline(t.trim_start_matches('#'));
            if !text.is_empty() {
                blocks.push(MdBlock::Heading { level, text });
            }
        } else if let Some(rest) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        {
            flush!();
            let (text, _) = extract_inline(rest);
            blocks.push(MdBlock::ListItem { text });
        } else if let Some(rest) = t.strip_prefix('>') {
            flush!();
            let (text, _) = extract_inline(rest);
            blocks.push(MdBlock::Quote { text });
        } else {
            let (text, links) = extract_inline(t);
            if !para.is_empty() {
                para.push(' ');
            }
            para.push_str(&text);
            para_links.extend(links);
        }
    }
    if in_code && !code.is_empty() {
        blocks.push(MdBlock::Code {
            text: code.trim_end().to_string(),
        });
    }
    flush!();
    blocks
}

// ---------------------------------------------------------------------------------------------
// Project page
// ---------------------------------------------------------------------------------------------

/// Everything loaded for the project page.
#[derive(Debug, Clone)]
pub struct ProjectPage {
    pub details: ProjectDetails,
    pub versions: Vec<ProjectVersion>,
}

/// State of the open project page.
#[derive(Default)]
pub struct ProjectViewState {
    pub open: Option<CatalogHit>,
    pub page: Option<ProjectPage>,
    pub blocks: Vec<MdBlock>,
    pub loading: bool,
    pub error: Option<String>,
    pub request_id: u64,
    pub filter_mc: String,
    pub filter_loader: Option<ModLoaderKind>,
    pub gallery_index: usize,
    pub selected_version: Option<usize>,
}

impl ProjectViewState {
    pub fn begin(&mut self, hit: CatalogHit) -> u64 {
        self.request_id += 1;
        self.open = Some(hit);
        self.page = None;
        self.blocks.clear();
        self.loading = true;
        self.error = None;
        self.gallery_index = 0;
        self.selected_version = None;
        self.request_id
    }

    pub fn close(&mut self) {
        self.request_id += 1;
        self.open = None;
        self.page = None;
        self.blocks.clear();
        self.loading = false;
        self.error = None;
    }

    /// Applies a worker result; stale results are ignored.
    pub fn apply(&mut self, request_id: u64, result: Result<ProjectPage, String>) -> bool {
        if request_id != self.request_id || self.open.is_none() {
            return false;
        }
        self.loading = false;
        match result {
            Ok(page) => {
                let src = page.details.body.clone().unwrap_or_default();
                self.blocks = parse_description(&src);
                self.page = Some(page);
            }
            Err(e) => self.error = Some(e),
        }
        true
    }

    /// Indices of versions matching the MC-version / loader filters.
    pub fn filtered_versions(&self) -> Vec<usize> {
        let Some(page) = &self.page else {
            return Vec::new();
        };
        let mc = self.filter_mc.trim();
        page.versions
            .iter()
            .enumerate()
            .filter(|(_, v)| mc.is_empty() || v.game_versions.iter().any(|g| g == mc))
            .filter(|(_, v)| {
                self.filter_loader.is_none_or(|l| {
                    v.loaders.is_empty()
                        || v.loaders.iter().any(|x| x.eq_ignore_ascii_case(l.as_str()))
                })
            })
            .map(|(i, _)| i)
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Install dialog
// ---------------------------------------------------------------------------------------------

/// Where the content goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallTarget {
    Existing(InstanceId),
    NewInstance { name: String },
}

/// Summary shown before a modpack becomes a new instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModpackPreview {
    pub mc_version: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub file_count: usize,
    pub total_size: u64,
    pub blocked: Vec<BlockedDownload>,
    pub archive_path: std::path::PathBuf,
}

/// Install dialog model.
pub struct InstallDialog {
    pub hit: CatalogHit,
    /// Additional queued projects for batch installation (excludes `hit`).
    pub batch_hits: Vec<CatalogHit>,
    /// Chosen modpack version (content installs let the resolver pick the latest compatible one).
    pub version: Option<ProjectVersion>,
    pub target: InstallTarget,
    pub request_id: u64,
    /// Display titles for dependency project ids.
    pub titles: std::collections::HashMap<String, String>,
    /// Source repository per project id for batch downloads.
    pub sources: std::collections::HashMap<String, mq_mods::CatalogSource>,
    pub plan: Option<DependencyResolutionPlan>,
    pub preview: Option<ModpackPreview>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_optionals: BTreeSet<String>,
    pub confirm_modpack: bool,
}

impl InstallDialog {
    pub fn new(hit: CatalogHit, version: Option<ProjectVersion>, target: InstallTarget) -> Self {
        Self {
            hit,
            batch_hits: Vec::new(),
            version,
            target,
            request_id: 0,
            titles: std::collections::HashMap::new(),
            sources: std::collections::HashMap::new(),
            plan: None,
            preview: None,
            loading: false,
            error: None,
            selected_optionals: BTreeSet::new(),
            confirm_modpack: false,
        }
    }

    /// Creates a batch install dialog resolving all `hits` (and their dependencies) at once.
    pub fn new_batch(mut hits: Vec<CatalogHit>, target: InstallTarget) -> Option<Self> {
        if hits.is_empty() {
            return None;
        }
        let primary = hits.remove(0);
        let mut titles = std::collections::HashMap::new();
        let mut sources = std::collections::HashMap::new();
        titles.insert(primary.project_id.clone(), primary.title.clone());
        sources.insert(primary.project_id.clone(), primary.source);
        for extra in &hits {
            titles.insert(extra.project_id.clone(), extra.title.clone());
            sources.insert(extra.project_id.clone(), extra.source);
        }
        Some(Self {
            hit: primary,
            batch_hits: hits,
            version: None,
            target,
            request_id: 0,
            titles,
            sources,
            plan: None,
            preview: None,
            loading: false,
            error: None,
            selected_optionals: BTreeSet::new(),
            confirm_modpack: false,
        })
    }

    /// All requested projects (primary + extra batch items).
    pub fn all_hits(&self) -> Vec<&CatalogHit> {
        let mut out = Vec::with_capacity(1 + self.batch_hits.len());
        out.push(&self.hit);
        out.extend(self.batch_hits.iter());
        out
    }

    pub fn is_batch(&self) -> bool {
        !self.batch_hits.is_empty()
    }

    pub fn is_modpack(&self) -> bool {
        self.hit.project_kind == ProjectKind::Modpack
    }

    /// Installation is blocked while loading, on error, or if the plan has no downloads.
    pub fn can_confirm(&self) -> bool {
        if self.loading || self.error.is_some() {
            return false;
        }
        if self.is_modpack() {
            return self.preview.is_some() && self.confirm_modpack;
        }
        self.plan.as_ref().is_some_and(|p| !p.downloads.is_empty())
    }

    pub fn toggle_optional(&mut self, project_id: &str) {
        if !self.selected_optionals.remove(project_id) {
            self.selected_optionals.insert(project_id.to_string());
        }
    }
}

/// One installed file that has a newer version at its source.
#[derive(Debug, Clone)]
pub struct UpdateCandidate {
    pub source: mq_mods::CatalogSource,
    pub kind: ProjectKind,
    pub project_id: String,
    pub old_filename: String,
    pub old_version_id: String,
    pub new: mq_mods::ResolvedModDownload,
}

/// State of the "check for updates" panel of the instance mods tab.
#[derive(Default)]
pub struct UpdatesState {
    pub instance: Option<InstanceId>,
    pub checking: bool,
    pub checked: bool,
    pub items: Vec<UpdateCandidate>,
    pub error: Option<String>,
    pub request_id: u64,
}

/// Human-readable size (`12.3 МБ`).
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.2} ГБ", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} МБ", b / (KB * KB))
    } else if b >= KB {
        format!("{:.0} КБ", b / KB)
    } else {
        format!("{bytes} Б")
    }
}

// ---------------------------------------------------------------------------------------------
// Downloads queue
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done,
    Failed(String),
    Cancelled,
}

/// One entry of the "Загрузки" queue.
#[derive(Clone)]
pub struct InstallJob {
    pub id: u64,
    pub title: String,
    pub target_label: String,
    pub done_files: usize,
    pub total_files: usize,
    pub status: JobStatus,
    pub cancel: Arc<AtomicBool>,
    /// Re-sendable request used by "Повторить".
    pub retry: Option<RetrySpec>,
}

/// Data needed to restart a failed/cancelled job.
#[derive(Debug, Clone)]
pub enum RetrySpec {
    Content {
        instance_id: InstanceId,
        kind: ProjectKind,
        source: mq_mods::CatalogSource,
        downloads: Vec<mq_mods::ResolvedModDownload>,
        blocked: Vec<BlockedDownload>,
    },
    Modpack {
        instance_name: String,
        preview: ModpackPreview,
    },
}

impl InstallJob {
    pub fn progress(&self) -> f32 {
        if self.total_files == 0 {
            0.0
        } else {
            (self.done_files as f32 / self.total_files as f32).clamp(0.0, 1.0)
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == JobStatus::Running
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Queue of install jobs.
#[derive(Default)]
pub struct JobQueue {
    pub jobs: Vec<InstallJob>,
    next_id: u64,
}

impl JobQueue {
    pub fn push(
        &mut self,
        title: String,
        target_label: String,
        total_files: usize,
        retry: Option<RetrySpec>,
    ) -> (u64, Arc<AtomicBool>) {
        self.next_id += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        self.jobs.push(InstallJob {
            id: self.next_id,
            title,
            target_label,
            done_files: 0,
            total_files,
            status: JobStatus::Running,
            cancel: cancel.clone(),
            retry,
        });
        (self.next_id, cancel)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut InstallJob> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn progress(&mut self, id: u64, done: usize, total: usize) {
        if let Some(j) = self.get_mut(id) {
            j.done_files = done;
            j.total_files = total.max(j.total_files);
        }
    }

    /// Final state of a job; a cancel request wins over a generic error.
    pub fn finish(&mut self, id: u64, result: Result<(), String>) {
        if let Some(j) = self.get_mut(id) {
            j.status = match result {
                Ok(()) => {
                    j.done_files = j.total_files;
                    JobStatus::Done
                }
                Err(_) if j.is_cancel_requested() => JobStatus::Cancelled,
                Err(e) => JobStatus::Failed(e),
            };
        }
    }

    /// Resets a failed/cancelled job to `Running` with a fresh cancel flag and returns the
    /// data needed to start it again.
    pub fn restart(&mut self, id: u64) -> Option<(Arc<AtomicBool>, RetrySpec)> {
        let j = self.get_mut(id)?;
        if !matches!(j.status, JobStatus::Failed(_) | JobStatus::Cancelled) {
            return None;
        }
        let spec = j.retry.clone()?;
        j.cancel = Arc::new(AtomicBool::new(false));
        j.status = JobStatus::Running;
        j.done_files = 0;
        Some((j.cancel.clone(), spec))
    }

    pub fn remove_finished(&mut self) {
        self.jobs.retain(InstallJob::is_active);
    }

    pub fn active_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.is_active()).count()
    }
}

// ---------------------------------------------------------------------------------------------
// Manual download dialog
// ---------------------------------------------------------------------------------------------

/// State of the "Эти файлы нужно скачать вручную" dialog.
#[derive(Default)]
pub struct ManualDownloadState {
    pub open: bool,
    pub instance_label: String,
    pub pending: Vec<BlockedDownload>,
    pub completed: Vec<String>,
    pub watch_dir_label: String,
    pub last_poll: Option<std::time::Instant>,
    pub message: Option<String>,
}

impl ManualDownloadState {
    pub fn remaining(&self) -> usize {
        self.pending.len()
    }

    /// Replaces the pending list after a watcher round and records newly completed names.
    pub fn apply_progress(&mut self, pending: Vec<BlockedDownload>, newly_done: Vec<String>) {
        self.pending = pending;
        self.completed.extend(newly_done);
        if self.pending.is_empty() {
            self.message = Some("Все файлы получены".to_string());
        }
    }
}

/// Default folder to watch for manual downloads (`~/Downloads`).
pub fn default_downloads_dir() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join("Downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_and_dangerous_markup_are_removed() {
        let out = sanitize_html(
            "Hi<script>alert(1)</script><style>x{}</style><iframe src=\"https://e\"></iframe> there<!-- c --><b>bold</b>",
        );
        assert!(!out.contains("alert") && !out.contains("iframe") && !out.contains("<"));
        assert!(out.contains("Hi there") && out.contains("bold"));
    }

    #[test]
    fn unsafe_urls_never_become_links_or_images() {
        let blocks = parse_description(
            "[x](javascript:alert(1)) [ok](https://example.com)\n\n![i](data:image/png;base64,AAA)\n\n<img src=\"file:///etc/passwd\"><img src=\"https://cdn.modrinth.com/a.png\" alt=\"A\">",
        );
        for b in &blocks {
            match b {
                MdBlock::Paragraph { links, .. } => {
                    assert!(links.iter().all(|(_, u)| is_safe_http_url(u)));
                    assert_eq!(links.len(), 1);
                }
                MdBlock::Image { url, .. } => assert!(url.starts_with("https://")),
                _ => {}
            }
        }
        assert_eq!(
            blocks
                .iter()
                .filter(|b| matches!(b, MdBlock::Image { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn markdown_structure_is_parsed() {
        let b = parse_description("# Title\n\nSome **bold** text\nsecond line\n\n- one\n- two\n\n```\ncode\n```\n---\n> quote");
        assert_eq!(
            b[0],
            MdBlock::Heading {
                level: 1,
                text: "Title".into()
            }
        );
        assert_eq!(
            b[1],
            MdBlock::Paragraph {
                text: "Some bold text second line".into(),
                links: vec![]
            }
        );
        assert!(matches!(b[2], MdBlock::ListItem { .. }));
        assert!(b.contains(&MdBlock::Code {
            text: "code".into()
        }));
        assert!(b.contains(&MdBlock::Rule));
        assert!(b.contains(&MdBlock::Quote {
            text: "quote".into()
        }));
    }

    #[test]
    fn html_description_keeps_headings_lists_and_links() {
        let b = parse_description(
            "<h2>Features</h2><ul><li>Fast</li><li>Small</li></ul><p>See <a href=\"https://x.org/p\">docs</a> &amp; more</p>",
        );
        assert_eq!(
            b[0],
            MdBlock::Heading {
                level: 2,
                text: "Features".into()
            }
        );
        assert_eq!(
            b.iter()
                .filter(|x| matches!(x, MdBlock::ListItem { .. }))
                .count(),
            2
        );
        let para = b.iter().find_map(|x| match x {
            MdBlock::Paragraph { text, links } => Some((text.clone(), links.clone())),
            _ => None,
        });
        let (text, links) = para.unwrap();
        assert_eq!(text, "See docs & more");
        assert_eq!(
            links,
            vec![("docs".to_string(), "https://x.org/p".to_string())]
        );
    }

    #[test]
    fn unterminated_tag_does_not_leak_markup() {
        assert!(!sanitize_html("text <img src=\"https://a/b.png\"").contains('<'));
    }

    #[test]
    fn size_formatting() {
        assert_eq!(format_size(10), "10 Б");
        assert_eq!(format_size(2048), "2 КБ");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 МБ");
    }

    #[test]
    fn job_queue_progress_cancel_and_cleanup() {
        let mut q = JobQueue::default();
        let (a, _) = q.push("A".into(), "inst".into(), 4, None);
        let (b, cancel_b) = q.push("B".into(), "inst".into(), 2, None);
        q.progress(a, 2, 4);
        assert!((q.jobs[0].progress() - 0.5).abs() < f32::EPSILON);
        q.finish(a, Ok(()));
        assert_eq!(q.jobs[0].status, JobStatus::Done);
        assert_eq!(q.jobs[0].progress(), 1.0);
        q.jobs[1].request_cancel();
        assert!(cancel_b.load(Ordering::SeqCst));
        q.finish(b, Err("aborted".into()));
        assert_eq!(q.jobs[1].status, JobStatus::Cancelled);
        assert_eq!(q.active_count(), 0);
        let (c, _) = q.push("C".into(), "i".into(), 1, None);
        q.finish(c, Err("net".into()));
        assert_eq!(q.jobs[2].status, JobStatus::Failed("net".into()));
        let spec = RetrySpec::Modpack {
            instance_name: "N".into(),
            preview: ModpackPreview {
                mc_version: "1.21.4".into(),
                loader: ModLoaderKind::Fabric,
                loader_version: None,
                file_count: 1,
                total_size: 1,
                blocked: vec![],
                archive_path: "a.mrpack".into(),
            },
        };
        let (d, _) = q.push("D".into(), "i".into(), 1, Some(spec));
        assert!(q.restart(d).is_none(), "running jobs cannot be restarted");
        q.finish(d, Err("net".into()));
        assert!(q.restart(c).is_none(), "no retry data");
        let (flag, _) = q.restart(d).unwrap();
        assert!(!flag.load(Ordering::SeqCst));
        assert_eq!(q.get_mut(d).unwrap().status, JobStatus::Running);
        q.remove_finished();
        assert_eq!(q.jobs.len(), 1);
    }

    fn version(mc: &str, loader: &str) -> ProjectVersion {
        ProjectVersion {
            id: "v".into(),
            project_id: "p".into(),
            name: "n".into(),
            version_number: "1".into(),
            game_versions: vec![mc.into()],
            loaders: vec![loader.into()],
            files: vec![],
            dependencies: vec![],
        }
    }

    fn hit(kind: ProjectKind) -> CatalogHit {
        CatalogHit {
            source: mq_mods::CatalogSource::Modrinth,
            project_id: "p".into(),
            slug: "p".into(),
            title: "P".into(),
            description: String::new(),
            author: String::new(),
            downloads: 0,
            icon_url: None,
            project_kind: kind,
            loaders: vec![],
            game_versions: vec![],
            latest_version: None,
        }
    }

    #[test]
    fn project_page_filters_versions_and_drops_stale_results() {
        let mut s = ProjectViewState::default();
        let id1 = s.begin(hit(ProjectKind::Mod));
        let id2 = s.begin(hit(ProjectKind::Mod));
        let mut details = hit(ProjectKind::Mod).into_project_details();
        details.body = Some("# Hello".into());
        let page = ProjectPage {
            details,
            versions: vec![version("1.21.4", "fabric"), version("1.20.1", "forge")],
        };
        assert!(!s.apply(id1, Ok(page.clone())));
        assert!(s.apply(id2, Ok(page)));
        assert_eq!(s.blocks.len(), 1);
        assert_eq!(s.filtered_versions(), vec![0, 1]);
        s.filter_mc = "1.20.1".into();
        assert_eq!(s.filtered_versions(), vec![1]);
        s.filter_loader = Some(ModLoaderKind::Fabric);
        assert!(s.filtered_versions().is_empty());
        s.close();
        assert!(!s.apply(id2, Err("late".into())));
    }

    #[test]
    fn install_dialog_confirmation_rules() {
        let mut d = InstallDialog::new(
            hit(ProjectKind::Mod),
            None,
            InstallTarget::Existing(InstanceId::new("x").unwrap()),
        );
        assert!(!d.can_confirm(), "no plan yet");
        d.plan = Some(DependencyResolutionPlan::default());
        assert!(!d.can_confirm(), "empty plan");
        d.toggle_optional("a");
        d.toggle_optional("b");
        d.toggle_optional("a");
        assert_eq!(d.selected_optionals.iter().collect::<Vec<_>>(), ["b"]);

        let mut m = InstallDialog::new(
            hit(ProjectKind::Modpack),
            Some(version("1.21.4", "fabric")),
            InstallTarget::NewInstance { name: "N".into() },
        );
        m.preview = Some(ModpackPreview {
            mc_version: "1.21.4".into(),
            loader: ModLoaderKind::Fabric,
            loader_version: None,
            file_count: 3,
            total_size: 10,
            blocked: vec![],
            archive_path: "a.mrpack".into(),
        });
        assert!(!m.can_confirm(), "must be confirmed explicitly");
        m.confirm_modpack = true;
        assert!(m.can_confirm());
        m.error = Some("x".into());
        assert!(!m.can_confirm());
    }

    #[test]
    fn manual_state_counts_remaining() {
        let mut s = ManualDownloadState::default();
        let b = |n: &str| BlockedDownload {
            project_id: "1".into(),
            file_id: "2".into(),
            project_title: "T".into(),
            filename: n.into(),
            webpage_url: "https://www.curseforge.com/x".into(),
            expected_size: 0,
            expected_sha1: None,
            expected_fingerprint: None,
            target_rel_path: format!("mods/{n}"),
        };
        s.apply_progress(vec![b("a.jar"), b("b.jar")], vec![]);
        assert_eq!(s.remaining(), 2);
        s.apply_progress(vec![b("b.jar")], vec!["a.jar".into()]);
        assert_eq!(s.remaining(), 1);
        assert_eq!(s.completed, ["a.jar"]);
        s.apply_progress(vec![], vec!["b.jar".into()]);
        assert!(s.message.is_some());
    }
}
