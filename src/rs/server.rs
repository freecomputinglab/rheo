use crate::Result;
use crate::constants::HTML_EXT;
use axum::{
    Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::get,
};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tracing::{info, warn};

/// Server state shared across handlers
#[derive(Clone)]
pub struct ServerState {
    /// Broadcast channel for sending reload events to connected clients
    pub reload_tx: broadcast::Sender<()>,
    /// Directory containing HTML files to serve
    pub html_dir: PathBuf,
}

/// Start the web server on a given port
///
/// Returns a tuple of (server_handle, reload_sender, server_url)
pub async fn start_server(
    html_dir: PathBuf,
    port: u16,
) -> Result<(tokio::task::JoinHandle<()>, broadcast::Sender<()>, String)> {
    // Create broadcast channel for reload events
    let (reload_tx, _) = broadcast::channel(100);

    let state = ServerState {
        reload_tx: reload_tx.clone(),
        html_dir: html_dir.clone(),
    };

    // Build router
    let app = Router::new()
        .route("/events", get(sse_handler))
        .fallback(get(static_handler))
        .with_state(state);

    // Bind to address
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::RheoError::io(e, format!("binding to {}", addr)))?;

    let server_url = format!("http://localhost:{}", port);
    info!(url = %server_url, "web server started");

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            warn!(error = %e, "web server stopped with error");
        }
    });

    Ok((server_handle, reload_tx, server_url))
}

/// SSE handler for live reload events
async fn sse_handler(
    State(state): State<ServerState>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.reload_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(_) => Some(Ok(Event::default().event("reload").data("refresh"))),
        Err(_) => None, // Ignore lagged messages
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

/// Static file handler with HTML injection for live reload script
async fn static_handler(State(state): State<ServerState>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Determine the file to serve
    let file_path = if path.is_empty() || path.ends_with('/') {
        // Check for index.html
        let index_path = state.html_dir.join(path).join("index.html");
        if index_path.exists() {
            index_path
        } else {
            // Return directory listing
            return directory_listing(&state.html_dir, path).into_response();
        }
    } else {
        state.html_dir.join(path)
    };

    // Check if file exists
    if !file_path.exists() {
        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    // Read file
    let content = match tokio::fs::read(&file_path).await {
        Ok(content) => content,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response();
        }
    };

    // If it's an HTML file, inject the live reload script
    if path.ends_with(HTML_EXT) {
        match inject_live_reload_script(&content) {
            Ok(modified_content) => {
                return Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::from(modified_content))
                    .unwrap();
            }
            Err(_) => {
                // If injection fails, serve original content
                warn!(file = ?file_path, "failed to inject live reload script");
            }
        }
    }

    // Serve file as-is for non-HTML or if injection failed
    let content_type = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(content))
        .unwrap()
}

/// Inject live reload script before </body> tag
fn inject_live_reload_script(html: &[u8]) -> std::io::Result<Vec<u8>> {
    let html_str = String::from_utf8_lossy(html);

    const SCRIPT: &str = r#"
<script>
const eventSource = new EventSource('/events');
eventSource.addEventListener('reload', function(e) {
    console.log('Reloading page...');
    location.reload();
});
eventSource.onerror = function(e) {
    console.log('SSE connection error, will retry automatically');
};
</script>
"#;

    // Try to inject before </body>, fall back to end of document
    let modified = if let Some(pos) = html_str.rfind("</body>") {
        let mut result = String::with_capacity(html_str.len() + SCRIPT.len());
        result.push_str(&html_str[..pos]);
        result.push_str(SCRIPT);
        result.push_str(&html_str[pos..]);
        result
    } else {
        let mut result = String::with_capacity(html_str.len() + SCRIPT.len());
        result.push_str(&html_str);
        result.push_str(SCRIPT);
        result
    };

    Ok(modified.into_bytes())
}

/// Generate a simple directory listing HTML page
fn directory_listing(html_dir: &Path, path: &str) -> Response {
    let dir_path = html_dir.join(path);

    let entries = match std::fs::read_dir(&dir_path) {
        Ok(entries) => entries,
        Err(_) => return (StatusCode::NOT_FOUND, "Directory not found").into_response(),
    };

    let mut html_files: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()? == "html" {
                Some(path.file_name()?.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();

    html_files.sort();

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>rheo - HTML Output</title>
    <style>
        body { font-family: system-ui, sans-serif; max-width: 800px; margin: 40px auto; padding: 0 20px; }
        h1 { color: #333; }
        ul { list-style: none; padding: 0; }
        li { margin: 10px 0; }
        a { color: #0066cc; text-decoration: none; padding: 8px 12px; display: inline-block; border-radius: 4px; }
        a:hover { background: #f0f0f0; }
    </style>
</head>
<body>
    <h1>HTML Output Files</h1>
    <ul>
"#,
    );

    if html_files.is_empty() {
        html.push_str("<li>No HTML files found</li>");
    } else {
        for file in html_files {
            html.push_str(&format!(
                r#"        <li><a href="{}">{}</a></li>
"#,
                file, file
            ));
        }
    }

    html.push_str(
        r#"    </ul>
</body>
</html>"#,
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}

/// Open a URL in the default browser
pub fn open_browser(url: &str) -> Result<()> {
    info!(url = %url, "opening browser");
    webbrowser::open(url)
        .map_err(|e| crate::RheoError::project_config(format!("failed to open browser: {}", e)))
}
