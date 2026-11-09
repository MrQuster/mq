#![deny(clippy::await_holding_lock)]
//! Background (non-UI-thread) catalog work: multi-source search with stale-request
//! cancellation and disk-cached icon loading.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use mq_core::AppConfig;
use mq_mods::{
    CatalogHit, CatalogSource, CatalogSourceFilter, ContentProvider, CurseForgeClient,
    ModrinthClient, SearchQuery, MQ_USER_AGENT,
};

use crate::catalog_state::{
    icon_cache_name, merge_hits, CatalogErrorKind, CatalogPage, CatalogSourceError,
};

/// Max accepted icon payload (2 MiB) and decoded edge length.
const MAX_ICON_BYTES: usize = 2 * 1024 * 1024;
pub const ICON_EDGE: u32 = 96;
/// Decoded edge length for gallery images.
pub const GALLERY_EDGE: u32 = 512;
/// Hosts icons may be fetched from.
const ICON_HOSTS: &[&str] = &[
    "cdn.modrinth.com",
    "media.forgecdn.net",
    "media.curseforge.com",
    "mediafilez.forgecdn.net",
];

/// Searches one provider and classifies a failure into a [`CatalogSourceError`].
pub async fn search_source<P: ContentProvider>(
    provider: &P,
    query: &SearchQuery,
    config: &AppConfig,
) -> Result<Vec<CatalogHit>, CatalogSourceError> {
    provider
        .search(query)
        .await
        .map_err(|e| CatalogSourceError {
            source: provider.source(),
            kind: CatalogErrorKind::classify(&e),
            message: config.sanitize_cf_api_key(&e.to_string()),
        })
}

/// Combines per-source outcomes into one page. `has_next` is set when a source filled the page.
pub fn assemble_page(
    request_id: u64,
    filter: CatalogSourceFilter,
    modrinth: Option<Result<Vec<CatalogHit>, CatalogSourceError>>,
    curseforge: Option<Result<Vec<CatalogHit>, CatalogSourceError>>,
    query: &SearchQuery,
) -> CatalogPage {
    let mut errors = Vec::new();
    let mut take = |r: Option<Result<Vec<CatalogHit>, CatalogSourceError>>| match r {
        Some(Ok(hits)) => hits,
        Some(Err(e)) => {
            errors.push(e);
            Vec::new()
        }
        None => Vec::new(),
    };
    let m = take(modrinth);
    let c = take(curseforge);
    let full = |n: usize| n >= query.limit as usize;
    let has_next = full(m.len()) || full(c.len());
    let hits = match filter {
        CatalogSourceFilter::All => merge_hits(m, c, query.sort),
        _ => m.into_iter().chain(c).collect(),
    };
    CatalogPage {
        request_id,
        hits,
        errors,
        has_next,
    }
}

/// Runs the search for the selected source filter. Returns `None` if a newer request
/// superseded this one (before or after the network round trip).
pub async fn run_search(
    config: &AppConfig,
    request_id: u64,
    latest: &AtomicU64,
    filter: CatalogSourceFilter,
    query: &SearchQuery,
) -> Option<CatalogPage> {
    let is_stale = || latest.load(Ordering::SeqCst) != request_id;
    if is_stale() {
        return None;
    }

    let modrinth_fut = async {
        if !filter.includes(CatalogSource::Modrinth) {
            return None;
        }
        Some(match ModrinthClient::from_config(config) {
            Ok(c) => search_source(&c, query, config).await,
            Err(e) => Err(CatalogSourceError {
                source: CatalogSource::Modrinth,
                kind: CatalogErrorKind::classify(&e),
                message: e.to_string(),
            }),
        })
    };
    let curseforge_fut = async {
        if !filter.includes(CatalogSource::CurseForge) {
            return None;
        }
        // Without a key CurseForge is reported as `MissingKey`; Modrinth keeps working.
        if config.effective_cf_api_key().is_none() {
            return Some(Err(CatalogSourceError {
                source: CatalogSource::CurseForge,
                kind: CatalogErrorKind::MissingKey,
                message: String::new(),
            }));
        }
        Some(match CurseForgeClient::from_config(config) {
            Ok(c) => search_source(&c, query, config).await,
            Err(e) => Err(CatalogSourceError {
                source: CatalogSource::CurseForge,
                kind: CatalogErrorKind::classify(&e),
                message: config.sanitize_cf_api_key(&e.to_string()),
            }),
        })
    };

    let (m, c) = tokio::join!(modrinth_fut, curseforge_fut);
    if is_stale() {
        return None;
    }
    Some(assemble_page(request_id, filter, m, c, query))
}

fn icon_url_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https" && u.host_str().is_some_and(|h| ICON_HOSTS.contains(&h))
    })
}

/// Decodes raw image bytes into a thumbnail `ColorImage`.
pub fn decode_icon(bytes: &[u8], edge: u32) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    // `thumbnail` also upscales, so only shrink oversized images.
    let thumb = if img.width() > edge || img.height() > edge {
        img.thumbnail(edge, edge).to_rgba8()
    } else {
        img.to_rgba8()
    };
    let size = [thumb.width() as usize, thumb.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        thumb.as_raw(),
    ))
}

/// Loads an icon from the disk cache or downloads it (HTTPS, allow-listed hosts, size-capped)
/// and stores the original bytes in `cache_dir`.
pub async fn load_icon(cache_dir: &Path, url: &str, edge: u32) -> Option<egui::ColorImage> {
    if !icon_url_allowed(url) {
        return None;
    }
    let path = cache_dir.join(icon_cache_name(url));
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if let Some(img) = decode_icon(&bytes, edge) {
            return Some(img);
        }
    }
    let client = reqwest::Client::builder()
        .user_agent(MQ_USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if resp
        .content_length()
        .is_some_and(|n| n as usize > MAX_ICON_BYTES)
    {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > MAX_ICON_BYTES {
        return None;
    }
    let img = decode_icon(&bytes, edge)?;
    if tokio::fs::create_dir_all(cache_dir).await.is_ok() {
        let tmp = path.with_extension("part");
        if tokio::fs::write(&tmp, &bytes).await.is_ok() {
            let _ = tokio::fs::rename(&tmp, &path).await;
        }
    }
    Some(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_mods::ProjectKind;

    fn hit(source: CatalogSource, id: &str) -> CatalogHit {
        CatalogHit {
            source,
            project_id: id.into(),
            slug: id.into(),
            title: id.into(),
            description: String::new(),
            author: String::new(),
            downloads: 1,
            icon_url: None,
            project_kind: ProjectKind::Mod,
            loaders: Vec::new(),
            game_versions: Vec::new(),
            latest_version: None,
        }
    }

    fn cf_missing_key() -> CatalogSourceError {
        CatalogSourceError {
            source: CatalogSource::CurseForge,
            kind: CatalogErrorKind::MissingKey,
            message: String::new(),
        }
    }

    #[test]
    fn modrinth_works_without_curseforge_key() {
        let q = SearchQuery::new("x");
        let page = assemble_page(
            3,
            CatalogSourceFilter::All,
            Some(Ok(vec![hit(CatalogSource::Modrinth, "a")])),
            Some(Err(cf_missing_key())),
            &q,
        );
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.errors.len(), 1);
        assert!(!page.has_next);
    }

    #[test]
    fn full_page_sets_has_next() {
        let q = SearchQuery::new("x").with_limit(2);
        let hits = vec![
            hit(CatalogSource::Modrinth, "a"),
            hit(CatalogSource::Modrinth, "b"),
        ];
        let page = assemble_page(1, CatalogSourceFilter::Modrinth, Some(Ok(hits)), None, &q);
        assert!(page.has_next);
        assert!(page.errors.is_empty());
    }

    #[test]
    fn icon_hosts_are_allow_listed() {
        assert!(icon_url_allowed("https://cdn.modrinth.com/data/x/icon.png"));
        assert!(icon_url_allowed("https://media.forgecdn.net/avatars/1.png"));
        assert!(!icon_url_allowed("http://cdn.modrinth.com/x.png"));
        assert!(!icon_url_allowed("https://evil.example/x.png"));
        assert!(!icon_url_allowed("file:///etc/passwd"));
    }

    #[test]
    fn decode_icon_rejects_garbage_and_accepts_png() {
        assert!(decode_icon(b"not an image", ICON_EDGE).is_none());
        let mut buf = Vec::new();
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 200, 90, 255]));
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        let out = decode_icon(&buf, ICON_EDGE).unwrap();
        assert_eq!(out.size, [8, 8]);
    }

    #[tokio::test]
    async fn stale_request_is_cancelled_before_network() {
        let latest = AtomicU64::new(5);
        let cfg = AppConfig::default();
        let q = SearchQuery::new("x");
        let out = run_search(&cfg, 4, &latest, CatalogSourceFilter::All, &q).await;
        assert!(out.is_none());
    }
}
