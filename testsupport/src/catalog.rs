//! Modrinth API v2 and CurseForge API v1 mock handlers and catalog models.

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::payload::{sha1_hex, sha512_hex};
use crate::world::MockWorld;

#[derive(Debug, Default, Deserialize)]
pub struct ModrinthSearchQuery {
    pub query: Option<String>,
    pub facets: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ModrinthBatchIdsQuery {
    pub ids: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeSearchQuery {
    pub search_filter: Option<String>,
    pub class_id: Option<u32>,
    pub index: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone)]
struct MockCatalogItem {
    project_id: &'static str,
    slug: &'static str,
    title: &'static str,
    description: &'static str,
    project_type: &'static str,
    categories: &'static [&'static str],
    loaders: &'static [&'static str],
    game_versions: &'static [&'static str],
    client_side: &'static str,
    server_side: &'static str,
    version_id: &'static str,
    version_number: &'static str,
    downloads: u64,
}

const MODRINTH_ITEMS: [MockCatalogItem; 4] = [
    MockCatalogItem {
        project_id: "AANobbMI",
        slug: "sodium",
        title: "Sodium",
        description: "Modern rendering engine and client-side optimization mod for Minecraft",
        project_type: "mod",
        categories: &["fabric", "quilt", "optimization"],
        loaders: &["fabric", "quilt"],
        game_versions: &["1.21.1", "1.20.4"],
        client_side: "required",
        server_side: "unsupported",
        version_id: "ver-sodium-1211",
        version_number: "0.6.0+mc1.21.1",
        downloads: 42_500_000,
    },
    MockCatalogItem {
        project_id: "gvQqBUqZ",
        slug: "lithium",
        title: "Lithium",
        description: "General-purpose server and physics optimization mod for Minecraft",
        project_type: "mod",
        categories: &["fabric", "quilt", "optimization"],
        loaders: &["fabric", "quilt"],
        game_versions: &["1.21.1", "1.20.4"],
        client_side: "optional",
        server_side: "required",
        version_id: "ver-lithium-1211",
        version_number: "0.13.0+mc1.21.1",
        downloads: 31_200_000,
    },
    MockCatalogItem {
        project_id: "P7dR8mSH",
        slug: "fabric-api",
        title: "Fabric API",
        description: "Essential hooks and interoperability library for Fabric mods",
        project_type: "mod",
        categories: &["fabric", "library"],
        loaders: &["fabric", "quilt"],
        game_versions: &["1.21.1", "1.20.4"],
        client_side: "optional",
        server_side: "optional",
        version_id: "ver-fabric-api-1211",
        version_number: "0.102.0+1.21.1",
        downloads: 95_000_000,
    },
    MockCatalogItem {
        project_id: "MQUltra0",
        slug: "mq-ultra-pack",
        title: "MQ Ultra Pack",
        description: "High-concurrency benchmark and verification modpack for MQ Launcher",
        project_type: "modpack",
        categories: &["fabric", "optimization", "multiplayer"],
        loaders: &["fabric"],
        game_versions: &["1.21.1"],
        client_side: "required",
        server_side: "optional",
        version_id: "ver-mq-ultra-pack-1211",
        version_number: "1.0.0",
        downloads: 120_000,
    },
];

pub(crate) fn modrinth_search(world: &MockWorld, params: &ModrinthSearchQuery) -> Response {
    let q = params
        .query
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let parsed_facets: Option<Vec<Vec<String>>> = params
        .facets
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|raw| serde_json::from_str(raw).ok());

    let filtered: Vec<&MockCatalogItem> = MODRINTH_ITEMS
        .iter()
        .filter(|item| {
            if !q.is_empty() {
                let matches_text = item.slug.to_ascii_lowercase().contains(&q)
                    || item.title.to_ascii_lowercase().contains(&q)
                    || item.description.to_ascii_lowercase().contains(&q);
                if !matches_text {
                    return false;
                }
            }
            if let Some(facet_groups) = &parsed_facets {
                for or_group in facet_groups {
                    if !or_group.is_empty() && !or_group.iter().any(|f| item_matches_facet(item, f))
                    {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    let total_hits = filtered.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20).max(1);

    let hits: Vec<Value> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|item| render_modrinth_search_hit(world, item))
        .collect();

    Json(json!({
        "hits": hits,
        "offset": offset,
        "limit": limit,
        "total_hits": total_hits
    }))
    .into_response()
}

fn item_matches_facet(item: &MockCatalogItem, facet: &str) -> bool {
    let Some((key, val)) = facet.split_once(':') else {
        return true;
    };
    let val = val.trim_matches('"');
    match key.trim() {
        "project_type" => item.project_type.eq_ignore_ascii_case(val),
        "versions" | "game_versions" => item
            .game_versions
            .iter()
            .any(|v| v.eq_ignore_ascii_case(val)),
        "categories" | "loaders" => {
            item.categories.iter().any(|c| c.eq_ignore_ascii_case(val))
                || item.loaders.iter().any(|l| l.eq_ignore_ascii_case(val))
        }
        "project_id" => item.project_id == val || item.slug == val,
        _ => true,
    }
}

fn render_modrinth_search_hit(world: &MockWorld, item: &MockCatalogItem) -> Value {
    json!({
        "project_id": item.project_id,
        "project_type": item.project_type,
        "slug": item.slug,
        "author": "MQTeam",
        "title": item.title,
        "description": item.description,
        "categories": item.categories,
        "display_categories": item.categories,
        "versions": item.game_versions,
        "downloads": item.downloads,
        "follows": 10_000,
        "icon_url": format!("{}/ely/skins/{}.png", world.base_url(), item.slug),
        "date_created": "2024-01-01T00:00:00Z",
        "date_modified": "2024-09-01T00:00:00Z",
        "latest_version": item.version_id,
        "license": "MIT",
        "client_side": item.client_side,
        "server_side": item.server_side,
        "gallery": []
    })
}

pub(crate) fn modrinth_project(world: &MockWorld, id_or_slug: &str) -> Response {
    let Some(item) = find_modrinth_item(id_or_slug) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "description": format!("Project {id_or_slug} not found")
            })),
        )
            .into_response();
    };
    Json(render_modrinth_project(world, item)).into_response()
}

pub(crate) fn modrinth_projects_batch(world: &MockWorld, q: &ModrinthBatchIdsQuery) -> Response {
    let ids: Vec<String> = q
        .ids
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let list: Vec<Value> = ids
        .iter()
        .filter_map(|id| find_modrinth_item(id).map(|item| render_modrinth_project(world, item)))
        .collect();
    Json(list).into_response()
}

fn render_modrinth_project(world: &MockWorld, item: &MockCatalogItem) -> Value {
    json!({
        "id": item.project_id,
        "slug": item.slug,
        "project_type": item.project_type,
        "team": "team-mq",
        "title": item.title,
        "description": item.description,
        "body": format!("# {}\n\n{}", item.title, item.description),
        "published": "2024-01-01T00:00:00Z",
        "updated": "2024-09-01T00:00:00Z",
        "status": "approved",
        "client_side": item.client_side,
        "server_side": item.server_side,
        "downloads": item.downloads,
        "followers": 10_000,
        "categories": item.categories,
        "additional_categories": [],
        "game_versions": item.game_versions,
        "loaders": item.loaders,
        "versions": [item.version_id],
        "icon_url": format!("{}/ely/skins/{}.png", world.base_url(), item.slug),
        "license": {
            "id": "MIT",
            "name": "MIT License",
            "url": null
        }
    })
}

pub(crate) fn modrinth_project_versions(world: &MockWorld, id_or_slug: &str) -> Response {
    let Some(item) = find_modrinth_item(id_or_slug) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "description": format!("Project {id_or_slug} not found")
            })),
        )
            .into_response();
    };
    Json(vec![render_modrinth_version(world, item)]).into_response()
}

pub(crate) fn modrinth_version_by_id(world: &MockWorld, version_id: &str) -> Response {
    let Some(item) = MODRINTH_ITEMS.iter().find(|i| i.version_id == version_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "description": format!("Version {version_id} not found")
            })),
        )
            .into_response();
    };
    Json(render_modrinth_version(world, item)).into_response()
}

pub(crate) fn modrinth_versions_batch(world: &MockWorld, q: &ModrinthBatchIdsQuery) -> Response {
    let ids: Vec<String> = q
        .ids
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let list: Vec<Value> = ids
        .iter()
        .filter_map(|vid| {
            MODRINTH_ITEMS
                .iter()
                .find(|i| i.version_id == vid)
                .map(|item| render_modrinth_version(world, item))
        })
        .collect();
    Json(list).into_response()
}

fn find_modrinth_item(id_or_slug: &str) -> Option<&'static MockCatalogItem> {
    MODRINTH_ITEMS
        .iter()
        .find(|i| i.project_id == id_or_slug || i.slug.eq_ignore_ascii_case(id_or_slug))
}

fn render_modrinth_version(world: &MockWorld, item: &MockCatalogItem) -> Value {
    let (filename, rel_url, meta, dependencies) = match item.slug {
        "sodium" => (
            "sodium-0.6.0.jar",
            "download/catalog/sodium-0.6.0.jar",
            world.catalog.sodium,
            json!([
                {
                    "version_id": "ver-fabric-api-1211",
                    "project_id": "P7dR8mSH",
                    "file_name": null,
                    "dependency_type": "required"
                }
            ]),
        ),
        "fabric-api" => (
            "fabric-api-0.102.0.jar",
            "download/catalog/fabric-api-0.102.0.jar",
            world.catalog.fabric_api,
            json!([]),
        ),
        "lithium" => (
            "lithium-0.13.0.jar",
            "download/catalog/lithium-0.13.0.jar",
            world.catalog.lithium,
            json!([]),
        ),
        _ => (
            "mq-ultra-pack.mrpack",
            "download/modpack/mq-ultra-pack.mrpack",
            world.mrpack_meta,
            json!([]),
        ),
    };

    json!({
        "id": item.version_id,
        "project_id": item.project_id,
        "author_id": "mq-author",
        "featured": true,
        "name": format!("{} {}", item.title, item.version_number),
        "version_number": item.version_number,
        "changelog": "Deterministic synthetic release",
        "date_published": "2024-09-01T00:00:00Z",
        "downloads": item.downloads,
        "version_type": "release",
        "status": "listed",
        "game_versions": item.game_versions,
        "loaders": item.loaders,
        "dependencies": dependencies,
        "files": [
            {
                "hashes": {
                    "sha512": sha512_hex(&meta.sha512),
                    "sha1": sha1_hex(&meta.sha1)
                },
                "url": format!("{}/{rel_url}", world.base_url()),
                "filename": filename,
                "primary": true,
                "size": meta.size,
                "file_type": null
            }
        ]
    })
}

// ============================================================================
// CurseForge API v1
// ============================================================================

pub(crate) fn check_curseforge_api_key(headers: &HeaderMap) -> Option<Response> {
    let has_valid_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !has_valid_key {
        Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "Forbidden",
                    "message": "Missing or empty x-api-key header"
                })),
            )
                .into_response(),
        )
    } else {
        None
    }
}

pub(crate) fn curseforge_search(
    world: &MockWorld,
    headers: &HeaderMap,
    query: &CurseForgeSearchQuery,
) -> Response {
    if let Some(resp) = check_curseforge_api_key(headers) {
        return resp;
    }
    let filter = query
        .search_filter
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    let all_mods = [
        render_cf_mod(world, 238222),
        render_cf_mod(world, 394468),
        render_cf_mod(world, 306612),
        render_cf_mod(world, 900001),
    ];

    let filtered: Vec<Value> = all_mods
        .into_iter()
        .flatten()
        .filter(|m| {
            if let Some(cid) = query.class_id {
                if m.get("classId").and_then(Value::as_u64) != Some(cid as u64) {
                    return false;
                }
            }
            if !filter.is_empty() {
                let name = m
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let slug = m
                    .get("slug")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !name.contains(&filter) && !slug.contains(&filter) {
                    return false;
                }
            }
            true
        })
        .collect();

    let index = query.index.unwrap_or(0);
    let page_size = query.page_size.unwrap_or(20).max(1);
    let total_count = filtered.len();
    let page: Vec<Value> = filtered.into_iter().skip(index).take(page_size).collect();

    Json(json!({
        "data": page,
        "pagination": {
            "index": index,
            "pageSize": page_size,
            "resultCount": page.len(),
            "totalCount": total_count
        }
    }))
    .into_response()
}

pub(crate) fn curseforge_mod_by_id(
    world: &MockWorld,
    headers: &HeaderMap,
    mod_id: u32,
) -> Response {
    if let Some(resp) = check_curseforge_api_key(headers) {
        return resp;
    }
    match render_cf_mod(world, mod_id) {
        Some(m) => Json(json!({ "data": m })).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "NotFound", "message": "Mod not found" })),
        )
            .into_response(),
    }
}

pub(crate) fn curseforge_mod_files(
    world: &MockWorld,
    headers: &HeaderMap,
    mod_id: u32,
) -> Response {
    if let Some(resp) = check_curseforge_api_key(headers) {
        return resp;
    }
    let Some(file) = render_cf_file_for_mod(world, mod_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "NotFound", "message": "Mod not found" })),
        )
            .into_response();
    };
    Json(json!({
        "data": [file],
        "pagination": {
            "index": 0,
            "pageSize": 50,
            "resultCount": 1,
            "totalCount": 1
        }
    }))
    .into_response()
}

pub(crate) fn curseforge_mod_file_by_id(
    world: &MockWorld,
    headers: &HeaderMap,
    mod_id: u32,
    file_id: u32,
) -> Response {
    if let Some(resp) = check_curseforge_api_key(headers) {
        return resp;
    }
    let Some(file) = render_cf_file_for_mod(world, mod_id) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "NotFound" }))).into_response();
    };
    if file.get("id").and_then(Value::as_u64) != Some(file_id as u64) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "FileNotFound" })),
        )
            .into_response();
    }
    Json(json!({ "data": file })).into_response()
}

pub(crate) fn curseforge_mod_file_download_url(
    world: &MockWorld,
    headers: &HeaderMap,
    mod_id: u32,
    file_id: u32,
) -> Response {
    if let Some(resp) = check_curseforge_api_key(headers) {
        return resp;
    }
    let Some(file) = render_cf_file_for_mod(world, mod_id) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "NotFound" }))).into_response();
    };
    if file.get("id").and_then(Value::as_u64) != Some(file_id as u64) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "FileNotFound" })),
        )
            .into_response();
    }
    let url = file
        .get("downloadUrl")
        .and_then(Value::as_str)
        .unwrap_or("");
    Json(json!({ "data": url })).into_response()
}

fn render_cf_mod(world: &MockWorld, mod_id: u32) -> Option<Value> {
    let (name, slug, summary, class_id, file_id) = match mod_id {
        238222 => (
            "Just Enough Items (JEI)",
            "jei",
            "Item and recipe viewing mod for Minecraft",
            6,
            5001,
        ),
        394468 => (
            "Sodium",
            "sodium",
            "Modern rendering engine and client-side optimization mod",
            6,
            5002,
        ),
        306612 => (
            "Fabric API",
            "fabric-api",
            "Core API library for the Fabric toolchain",
            6,
            5003,
        ),
        900001 => (
            "MQ CurseForge Pack",
            "mq-cf-pack",
            "Synthetic CurseForge modpack for MQ tests",
            4471,
            5004,
        ),
        _ => return None,
    };

    let file = render_cf_file_for_mod(world, mod_id)?;
    let filename = file
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or("artifact.jar")
        .to_string();

    Some(json!({
        "id": mod_id,
        "gameId": 432,
        "name": name,
        "slug": slug,
        "classId": class_id,
        "summary": summary,
        "status": 4,
        "downloadCount": 1_500_000,
        "isFeatured": true,
        "primaryCategoryId": class_id,
        "authors": [
            { "id": 1, "name": "MQAuthor", "url": "https://example.invalid/mq" }
        ],
        "logo": {
            "id": mod_id,
            "modId": mod_id,
            "title": name,
            "url": format!("{}/ely/skins/{slug}.png", world.base_url())
        },
        "mainFileId": file_id,
        "latestFiles": [file],
        "latestFilesIndexes": [
            {
                "gameVersion": "1.21.1",
                "fileId": file_id,
                "filename": filename,
                "releaseType": 1,
                "modLoader": 4
            }
        ],
        "dateCreated": "2024-01-01T00:00:00Z",
        "dateModified": "2024-09-01T00:00:00Z",
        "dateReleased": "2024-09-01T00:00:00Z",
        "allowModDistribution": true
    }))
}

fn render_cf_file_for_mod(world: &MockWorld, mod_id: u32) -> Option<Value> {
    let (file_id, file_name, meta, deps) = match mod_id {
        238222 => (
            5001u32,
            "jei-1.21.1-19.8.0.jar",
            world.catalog.jei,
            json!([]),
        ),
        394468 => (
            5002u32,
            "sodium-0.6.0.jar",
            world.catalog.sodium,
            // relationType 3 = RequiredDependency in CurseForge API v1.
            json!([{ "modId": 306612, "relationType": 3 }]),
        ),
        306612 => (
            5003u32,
            "fabric-api-0.102.0.jar",
            world.catalog.fabric_api,
            json!([]),
        ),
        900001 => (
            5004u32,
            "mq-cf-pack.zip",
            world.cf_zip_meta,
            json!([
                { "modId": 394468, "relationType": 3 },
                { "modId": 306612, "relationType": 3 }
            ]),
        ),
        _ => return None,
    };

    let sha1 = sha1_hex(&meta.sha1);
    let download_url = format!(
        "{}/download/curseforge/{mod_id}/{file_id}/{file_name}",
        world.base_url()
    );

    Some(json!({
        "id": file_id,
        "gameId": 432,
        "modId": mod_id,
        "isAvailable": true,
        "displayName": file_name,
        "fileName": file_name,
        "releaseType": 1,
        "fileStatus": 4,
        "hashes": [
            {
                "value": sha1,
                "algo": 1
            }
        ],
        "fileDate": "2024-09-01T00:00:00Z",
        "fileLength": meta.size,
        "downloadCount": 500_000,
        "downloadUrl": download_url,
        "gameVersions": ["1.21.1", "1.20.4", "Fabric"],
        "dependencies": deps
    }))
}
