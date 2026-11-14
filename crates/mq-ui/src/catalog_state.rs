#![deny(clippy::await_holding_lock)]
//! Pure (UI-free, I/O-free) state model for the catalog screen: tabs, source filter,
//! 300 ms search debounce, stale-request cancellation, pagination, per-source errors
//! and lazily loaded icon slots.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mq_core::{ModLoaderKind, MqError};
use mq_mods::{
    CatalogHit, CatalogSource, CatalogSourceFilter, ProjectKind, SearchQuery, SearchSort,
};

/// Debounce between the last edit of a search/filter and the actual network request.
pub const CATALOG_DEBOUNCE: Duration = Duration::from_millis(300);
/// Results requested per source per page.
pub const CATALOG_PAGE_SIZE: u32 = 20;
/// Maximum number of icon downloads running at the same time.
pub const MAX_ICON_INFLIGHT: usize = 6;

/// Top-level tabs of the catalog screen.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CatalogTab {
    #[default]
    Mods,
    ResourcePacks,
    Shaders,
    Modpacks,
}

impl CatalogTab {
    pub const ALL: [CatalogTab; 4] = [
        CatalogTab::Mods,
        CatalogTab::ResourcePacks,
        CatalogTab::Shaders,
        CatalogTab::Modpacks,
    ];

    pub const fn kind(self) -> ProjectKind {
        match self {
            Self::Mods => ProjectKind::Mod,
            Self::ResourcePacks => ProjectKind::ResourcePack,
            Self::Shaders => ProjectKind::Shader,
            Self::Modpacks => ProjectKind::Modpack,
        }
    }

    pub const fn from_kind(kind: ProjectKind) -> Self {
        match kind {
            ProjectKind::Mod => Self::Mods,
            ProjectKind::ResourcePack => Self::ResourcePacks,
            ProjectKind::Shader => Self::Shaders,
            ProjectKind::Modpack => Self::Modpacks,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Mods => "Моды",
            Self::ResourcePacks => "Ресурспаки",
            Self::Shaders => "Шейдеры",
            Self::Modpacks => "Сборки",
        }
    }
}

/// Classified reason why one catalog source failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogErrorKind {
    Network,
    RateLimited,
    MissingKey,
    Other,
}

impl CatalogErrorKind {
    /// Maps a provider error onto the UI-level error class.
    pub fn classify(err: &MqError) -> Self {
        match err {
            MqError::HttpStatus { status: 429, .. } => Self::RateLimited,
            MqError::HttpStatus {
                status: 401 | 403, ..
            } => Self::MissingKey,
            MqError::Network(_) => Self::Network,
            MqError::HttpStatus { status, .. } if *status >= 500 => Self::Network,
            _ => Self::Other,
        }
    }
}

/// Failure of one catalog source (the other source may still deliver results).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSourceError {
    pub source: CatalogSource,
    pub kind: CatalogErrorKind,
    pub message: String,
}

/// Result of one worker search round.
#[derive(Debug, Clone, Default)]
pub struct CatalogPage {
    pub request_id: u64,
    pub hits: Vec<CatalogHit>,
    pub errors: Vec<CatalogSourceError>,
    pub has_next: bool,
}

/// Filters shown in the filter row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogFilters {
    pub mc_version: String,
    pub loader: Option<ModLoaderKind>,
    pub categories: Vec<String>,
    pub sort: SearchSort,
}

impl Default for CatalogFilters {
    fn default() -> Self {
        Self {
            mc_version: String::new(),
            loader: None,
            categories: Vec::new(),
            sort: SearchSort::Downloads,
        }
    }
}

/// Lazy icon slot; the image is uploaded to the GPU by the view and the slot marked `Uploaded`.
pub enum IconSlot {
    Pending,
    Ready(egui::ColorImage),
    Uploaded,
    Failed,
}

/// Complete catalog screen state.
pub struct CatalogState {
    pub tab: CatalogTab,
    pub source: CatalogSourceFilter,
    pub query: String,
    pub filters: CatalogFilters,
    pub page: u32,
    pub hits: Vec<CatalogHit>,
    pub errors: Vec<CatalogSourceError>,
    pub loading: bool,
    pub has_next: bool,
    /// `true` after the first completed response (distinguishes "empty" from "not searched yet").
    pub searched: bool,
    pub dirty_since: Option<Instant>,
    pub request_seq: u64,
    pub latest_request: Arc<AtomicU64>,
    pub icons: HashMap<String, IconSlot>,
}

impl Default for CatalogState {
    fn default() -> Self {
        Self {
            tab: CatalogTab::Mods,
            source: CatalogSourceFilter::All,
            query: String::new(),
            filters: CatalogFilters::default(),
            page: 0,
            hits: Vec::new(),
            errors: Vec::new(),
            loading: false,
            has_next: false,
            searched: false,
            dirty_since: None,
            request_seq: 0,
            latest_request: Arc::new(AtomicU64::new(0)),
            icons: HashMap::new(),
        }
    }
}

impl CatalogState {
    /// Marks the query as changed; resets to the first page and restarts the debounce timer.
    pub fn mark_dirty(&mut self, now: Instant) {
        self.page = 0;
        self.dirty_since = Some(now);
    }

    /// Marks the query as changed without resetting the page (pagination clicks).
    pub fn mark_page_changed(&mut self, now: Instant, debounce: bool) {
        self.dirty_since = Some(if debounce {
            now
        } else {
            now.checked_sub(CATALOG_DEBOUNCE).unwrap_or(now)
        });
    }

    /// Time left until the pending query should fire (`None` if nothing is pending).
    pub fn debounce_remaining(&self, now: Instant) -> Option<Duration> {
        self.dirty_since
            .map(|t| CATALOG_DEBOUNCE.saturating_sub(now.saturating_duration_since(t)))
    }

    /// `true` when a pending query exists and its debounce window has elapsed.
    pub fn is_due(&self, now: Instant) -> bool {
        self.debounce_remaining(now) == Some(Duration::ZERO)
    }

    /// Builds the provider query for the current UI state.
    pub fn build_query(&self) -> SearchQuery {
        let mc = self.filters.mc_version.trim();
        SearchQuery {
            query: self.query.trim().to_string(),
            project_kind: self.tab.kind(),
            mc_version: (!mc.is_empty()).then(|| mc.to_string()),
            loader: if self.tab == CatalogTab::Mods || self.tab == CatalogTab::Modpacks {
                self.filters.loader
            } else {
                None
            },
            categories: self.filters.categories.clone(),
            sort: self.filters.sort,
            limit: CATALOG_PAGE_SIZE,
            offset: self.page * CATALOG_PAGE_SIZE,
        }
    }

    /// Starts a new request: bumps the sequence (which invalidates every older request) and
    /// returns `(request_id, query)`.
    pub fn begin_request(&mut self) -> (u64, SearchQuery) {
        self.request_seq += 1;
        self.latest_request
            .store(self.request_seq, Ordering::SeqCst);
        self.dirty_since = None;
        self.loading = true;
        (self.request_seq, self.build_query())
    }

    /// Applies a worker response; stale responses (older than the latest request) are dropped.
    /// Returns `true` if the page was applied.
    pub fn apply_page(&mut self, page: CatalogPage) -> bool {
        if page.request_id != self.request_seq {
            return false;
        }
        self.hits = page.hits;
        self.errors = page.errors;
        self.has_next = page.has_next;
        self.loading = false;
        self.searched = true;
        true
    }

    /// `true` if every enabled source failed and nothing is shown.
    pub fn all_sources_failed(&self) -> bool {
        self.hits.is_empty() && !self.errors.is_empty()
    }

    /// Icon URLs of the given hits that still need a download, limited by the in-flight cap.
    pub fn icons_to_fetch<'a>(
        &mut self,
        hits: impl Iterator<Item = &'a CatalogHit>,
    ) -> Vec<String> {
        let mut inflight = self
            .icons
            .values()
            .filter(|s| matches!(s, IconSlot::Pending))
            .count();
        let mut out = Vec::new();
        for url in hits.filter_map(|h| h.icon_url.as_deref()) {
            if inflight >= MAX_ICON_INFLIGHT {
                break;
            }
            if url.starts_with("https://") && !self.icons.contains_key(url) {
                self.icons.insert(url.to_string(), IconSlot::Pending);
                out.push(url.to_string());
                inflight += 1;
            }
        }
        out
    }
}

/// Merges two source result lists for the "All" filter. Sorted by downloads for the
/// `Downloads` sort, otherwise interleaved to keep each source's own relevance order.
/// Duplicates (same title, case-insensitive, same project kind) keep the first occurrence.
pub fn merge_hits(
    modrinth: Vec<CatalogHit>,
    curseforge: Vec<CatalogHit>,
    sort: SearchSort,
) -> Vec<CatalogHit> {
    let mut merged: Vec<CatalogHit> = Vec::with_capacity(modrinth.len() + curseforge.len());
    if sort == SearchSort::Downloads {
        merged.extend(modrinth);
        merged.extend(curseforge);
        merged.sort_by_key(|h| std::cmp::Reverse(h.downloads));
    } else {
        let mut a = modrinth.into_iter();
        let mut b = curseforge.into_iter();
        loop {
            let (x, y) = (a.next(), b.next());
            if x.is_none() && y.is_none() {
                break;
            }
            merged.extend(x);
            merged.extend(y);
        }
    }
    let mut seen = std::collections::HashSet::new();
    merged.retain(|h| seen.insert((h.title.to_lowercase(), h.source, h.project_id.clone())));
    merged
}

/// FNV-1a 64-bit hash (stable across Rust versions) used for icon cache file names.
pub fn icon_cache_name(url: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}.img")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(source: CatalogSource, id: &str, title: &str, downloads: u64) -> CatalogHit {
        CatalogHit {
            source,
            project_id: id.into(),
            slug: id.into(),
            title: title.into(),
            description: String::new(),
            author: String::new(),
            downloads,
            icon_url: Some(format!("https://cdn.modrinth.com/{id}.png")),
            project_kind: ProjectKind::Mod,
            loaders: Vec::new(),
            game_versions: Vec::new(),
            latest_version: None,
        }
    }

    #[test]
    fn debounce_fires_only_after_300ms() {
        let mut s = CatalogState::default();
        let t0 = Instant::now();
        assert!(!s.is_due(t0));
        s.mark_dirty(t0);
        assert!(!s.is_due(t0 + Duration::from_millis(299)));
        assert!(s.is_due(t0 + Duration::from_millis(300)));
        // typing again restarts the window
        s.mark_dirty(t0 + Duration::from_millis(250));
        assert!(!s.is_due(t0 + Duration::from_millis(400)));
        assert!(s.is_due(t0 + Duration::from_millis(550)));
    }

    #[test]
    fn stale_responses_are_dropped() {
        let mut s = CatalogState::default();
        let (first, _) = s.begin_request();
        let (second, _) = s.begin_request();
        assert_eq!(s.latest_request.load(Ordering::SeqCst), second);
        let stale = CatalogPage {
            request_id: first,
            hits: vec![hit(CatalogSource::Modrinth, "a", "A", 1)],
            ..Default::default()
        };
        assert!(!s.apply_page(stale));
        assert!(s.hits.is_empty() && s.loading);
        let fresh = CatalogPage {
            request_id: second,
            hits: vec![hit(CatalogSource::Modrinth, "b", "B", 1)],
            has_next: true,
            ..Default::default()
        };
        assert!(s.apply_page(fresh));
        assert_eq!(s.hits.len(), 1);
        assert!(!s.loading && s.searched && s.has_next);
    }

    #[test]
    fn query_uses_page_offset_and_tab_kind() {
        let mut s = CatalogState {
            tab: CatalogTab::Shaders,
            ..Default::default()
        };
        s.filters.loader = Some(ModLoaderKind::Fabric);
        s.filters.mc_version = " 1.21.4 ".into();
        s.page = 2;
        let q = s.build_query();
        assert_eq!(q.project_kind, ProjectKind::Shader);
        assert_eq!(q.offset, 2 * CATALOG_PAGE_SIZE);
        assert_eq!(q.mc_version.as_deref(), Some("1.21.4"));
        assert_eq!(q.loader, None, "shaders ignore the loader filter");
        s.mark_dirty(Instant::now());
        assert_eq!(s.page, 0, "editing the query resets pagination");
    }

    #[test]
    fn error_classification() {
        let rl = MqError::HttpStatus {
            status: 429,
            url: "u".into(),
            body: String::new(),
        };
        assert_eq!(
            CatalogErrorKind::classify(&rl),
            CatalogErrorKind::RateLimited
        );
        assert_eq!(
            CatalogErrorKind::classify(&MqError::Network("x".into())),
            CatalogErrorKind::Network
        );
        let forbidden = MqError::HttpStatus {
            status: 403,
            url: "u".into(),
            body: String::new(),
        };
        assert_eq!(
            CatalogErrorKind::classify(&forbidden),
            CatalogErrorKind::MissingKey
        );
    }

    #[test]
    fn merge_sorts_by_downloads_or_interleaves() {
        let m = vec![
            hit(CatalogSource::Modrinth, "m1", "Alpha", 10),
            hit(CatalogSource::Modrinth, "m2", "Beta", 5),
        ];
        let c = vec![hit(CatalogSource::CurseForge, "c1", "Gamma", 50)];
        let by_dl = merge_hits(m.clone(), c.clone(), SearchSort::Downloads);
        assert_eq!(by_dl[0].project_id, "c1");
        let inter = merge_hits(m, c, SearchSort::Relevance);
        let ids: Vec<_> = inter.iter().map(|h| h.project_id.as_str()).collect();
        assert_eq!(ids, ["m1", "c1", "m2"]);
    }

    #[test]
    fn icon_fetch_is_capped_and_deduplicated() {
        let mut s = CatalogState::default();
        let hits: Vec<_> = (0..10)
            .map(|i| hit(CatalogSource::Modrinth, &format!("p{i}"), "T", 1))
            .collect();
        let first = s.icons_to_fetch(hits.iter());
        assert_eq!(first.len(), MAX_ICON_INFLIGHT);
        assert!(s.icons_to_fetch(hits.iter()).is_empty(), "cap reached");
        s.icons.insert(first[0].clone(), IconSlot::Failed);
        assert_eq!(s.icons_to_fetch(hits.iter()).len(), 1);
    }

    #[test]
    fn icon_cache_name_is_stable() {
        assert_eq!(icon_cache_name("a"), icon_cache_name("a"));
        assert_ne!(icon_cache_name("a"), icon_cache_name("b"));
        assert_eq!(icon_cache_name("").len(), 16 + 4);
    }
}
