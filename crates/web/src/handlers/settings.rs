use crate::handlers::WebState;
use crate::models::{CodeEditorLibrary, RedSystemSettings};
use axum::extract::{Path, Query};
use axum::{
    Extension,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Json},
};
use edgelink_core::runtime::paths;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

// settings/user_settings/locale/icons related handlers
// ...existing code...

/// Get user settings
pub async fn get_user_settings(Extension(_state): Extension<Arc<WebState>>) -> Result<Json<Value>, StatusCode> {
    let settings = serde_json::json!({
        "user": "default",
        "theme": "default",
        "language": "en-US"
    });

    Ok(Json(settings))
}

/// Update user settings
pub async fn update_user_settings(
    Extension(_state): Extension<Arc<WebState>>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    log::debug!("Updating user settings: {payload:?}");

    // In actual implementation, this should save the settings
    Ok(Json(payload))
}
/// Get system settings
pub async fn get_settings(Extension(state): Extension<Arc<WebState>>) -> Result<Json<RedSystemSettings>, StatusCode> {
    let settings = state.red_settings.as_ref();
    Ok(Json(settings.clone()))
}

/// List the icons available from this editor's static directory.
pub async fn get_icons() -> Result<Json<BTreeMap<String, Vec<String>>>, StatusCode> {
    tokio::task::spawn_blocking(|| editor_icons(&paths::ui_static_dir().join("icons")))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn editor_icons(root: &std::path::Path) -> std::io::Result<BTreeMap<String, Vec<String>>> {
    let mut icons = BTreeMap::new();
    let modules = match std::fs::read_dir(root) {
        Ok(modules) => modules,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(icons),
        Err(error) => return Err(error),
    };
    for module in modules {
        let module = module?;
        if !module.file_type()?.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        for file in std::fs::read_dir(module.path())? {
            let file = file?;
            let name = file.file_name();
            if file.file_type()?.is_file()
                && let Some(name) = name.to_str()
                && !name.starts_with('.')
                && matches!(name.rsplit_once('.'), Some((_, "svg" | "png" | "gif" | "jpg" | "jpeg")))
            {
                files.push(name.to_owned());
            }
        }
        if !files.is_empty()
            && let Ok(name) = module.file_name().into_string()
            && !name.starts_with('.')
        {
            files.sort_unstable();
            icons.insert(name, files);
        }
    }
    Ok(icons)
}

#[cfg(test)]
#[test]
fn icon_index_matches_installed_files() {
    let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    assert!(editor_icons(&root).unwrap().is_empty());
    for (module, name) in
        [("node-red", "bridge.svg"), ("node-red", "alert.svg"), ("can", "port.png"), ("can", "LICENSE")]
    {
        let dir = root.join(module);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), []).unwrap();
    }
    let icons = editor_icons(&root).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(icons["node-red"], ["alert.svg", "bridge.svg"]);
    assert_eq!(icons["can"], ["port.png"]);
}

/// Get icon file
pub async fn get_icon_file(
    Path((module, icon)): Path<(String, String)>,
) -> Result<axum::response::Response, StatusCode> {
    log::debug!("Requesting icon: {icon} from module: {module}");

    // Get static file directory
    let static_dir = paths::ui_static_dir();

    // Build icon file path
    let icon_path = static_dir.join("icons").join(&module).join(&icon);

    // Security check - ensure we do not escape the static directory
    if !icon_path.starts_with(&static_dir) {
        log::warn!("Attempted path traversal attack: {}", icon_path.display());
        return Err(StatusCode::FORBIDDEN);
    }

    // Try to read the icon file
    match tokio::fs::read(&icon_path).await {
        Ok(content) => {
            let mut headers = axum::http::HeaderMap::new();

            // Set correct Content-Type based on file extension
            if icon.ends_with(".svg") {
                headers.insert(axum::http::header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
            } else if icon.ends_with(".png") {
                headers.insert(axum::http::header::CONTENT_TYPE, "image/png".parse().unwrap());
            } else if icon.ends_with(".jpg") || icon.ends_with(".jpeg") {
                headers.insert(axum::http::header::CONTENT_TYPE, "image/jpeg".parse().unwrap());
            } else if icon.ends_with(".gif") {
                headers.insert(axum::http::header::CONTENT_TYPE, "image/gif".parse().unwrap());
            } else {
                // Default to SVG
                headers.insert(axum::http::header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
            }

            Ok((headers, content).into_response())
        }
        Err(_) => {
            log::warn!("Icon not found: {}", icon_path.display());
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Get Plugins
pub async fn get_plugins(headers: HeaderMap) -> Result<axum::response::Response, StatusCode> {
    // Check Accept header to determine response format
    let accept_header = headers.get("accept").and_then(|h| h.to_str().ok()).unwrap_or("application/json");

    if accept_header.contains("text/html") {
        // Return HTML config for plugins
        let html_content = generate_plugins_html().await;
        Ok(Html(html_content).into_response())
    } else {
        // Return plugin list in JSON format
        let plugins = serde_json::json!([]);
        Ok(Json(plugins).into_response())
    }
}

/// Generate HTML config for plugins
async fn generate_plugins_html() -> String {
    // Node-RED frontend expects plugin config in HTML format
    // Each plugin is wrapped with specific comment delimiters
    // Currently returns an empty string, meaning no plugins
    "".to_string()
}

pub async fn get_theme(Extension(state): Extension<Arc<WebState>>) -> Result<Json<Value>, StatusCode> {
    // Return Node-RED compatible theme list
    let mut jd = serde_json::json!({
        "page": {
            "title": "EdgeLinkd",
            "favicon": "favicon.ico",
            "tabicon": {
                "icon": "red/images/node-red-icon-black.svg",
                "colour": "#8f0000"
            }
        },
        "header": {
            "title": "EdgeLinkd",
            "image": "red/images/node-red.svg"
        },
        "asset": {
            "red": "red/red.min.js",
            "main": "red/main.min.js",
            "vendorMonaco": "",
            "vendorAce": ""
        },
        "themes": []
    });
    match state.red_settings.editor_theme.code_editor.lib {
        CodeEditorLibrary::Monaco => jd["asset"]["vendorMonaco"] = "vendor/monaco/monaco-bootstrap.js".into(),
        CodeEditorLibrary::Ace => jd["asset"]["vendorAce"] = "vendor/ace/ace-bootstrap.js".into(),
        CodeEditorLibrary::Basic => {}
    }
    Ok(Json(jd))
}

/// Third-party login is not offered by EdgeLinkd; never fabricate an access token.
pub async fn exchange_auth_code() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "not_supported",
            "message": "Third-party authentication code exchange is not supported by EdgeLinkd"
        })),
    )
}

/// Select the bootstrap matching the code editor advertised in /settings.
pub async fn get_editor_index(Extension(state): Extension<Arc<WebState>>) -> Result<Html<String>, StatusCode> {
    let mut html = tokio::fs::read_to_string(state.static_dir.join("index.html"))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !matches!(state.red_settings.editor_theme.code_editor.lib, CodeEditorLibrary::Monaco) {
        html = html.replace("<link rel=\"stylesheet\" href=\"vendor/monaco/style.css\">", "");
        let bootstrap = match state.red_settings.editor_theme.code_editor.lib {
            CodeEditorLibrary::Ace => "<script id=\"ace-bootstrap\" src=\"vendor/ace/ace-bootstrap.js\"></script>",
            _ => "",
        };
        html = html
            .replace("<script id=\"monaco-bootstrap\" src=\"vendor/monaco/monaco-bootstrap.js\"></script>", bootstrap);
    }
    Ok(Html(html))
}
/// Get plugin messages
pub async fn get_plugin_messages(Query(params): Query<HashMap<String, String>>) -> Result<Json<Value>, StatusCode> {
    let lang = params.get("lng").unwrap_or(&"en-US".to_string()).clone();

    log::debug!("Getting plugin messages for language: {lang}");

    // Return localized messages for plugins
    let messages = match lang.as_str() {
        "zh-CN" => serde_json::json!({
            "edgelink": {
                "plugin": {
                    "name": "EdgeLinkd 插件",
                    "description": "EdgeLinkd 核心插件",
                    "version": "版本"
                }
            }
        }),
        _ => serde_json::json!({
            "edgelink": {
                "plugin": {
                    "name": "EdgeLinkd Plugin",
                    "description": "EdgeLinkd core plugin",
                    "version": "Version"
                }
            }
        }),
    };

    Ok(Json(messages))
}
