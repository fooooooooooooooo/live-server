use std::error::Error;
use std::io::ErrorKind;
use std::path::Path;
use std::{fs, net::IpAddr};

use axum::{
    body::Body,
    extract::{ws::Message, Request, WebSocketUpgrade},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use local_ip_address::local_ip;
use std::future::Future;
use tokio::net::TcpListener;

use crate::listing::serve_directory_listing;
use crate::static_files::{
    get_dir_link_svg, get_dir_svg, get_entry_html, get_file_link_svg, get_file_svg, get_index_css,
    get_listing_html, get_unknown_svg,
};
use crate::{ADDR, ROOT, TX, WATCH};

pub(crate) async fn serve(tcp_listener: TcpListener, router: Router) {
    axum::serve(tcp_listener, router).await.unwrap();
}

pub(crate) async fn create_listener(addr: String) -> Result<TcpListener, String> {
    match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            let port = listener.local_addr().unwrap().port();
            let host = listener.local_addr().unwrap().ip();
            let host = match host.is_unspecified() {
                true => match local_ip() {
                    Ok(addr) => addr,
                    Err(err) => {
                        log::warn!("Failed to get local IP address: {}", err);
                        host
                    }
                },
                false => host,
            };

            let addr = match host {
                IpAddr::V4(host) => format!("{host}:{port}"),
                IpAddr::V6(host) => format!("[{host}]:{port}"),
            };
            log::info!("Listening on http://{addr}/");
            ADDR.set(addr).unwrap();
            Ok(listener)
        }
        Err(err) => {
            let err_msg = if let std::io::ErrorKind::AddrInUse = err.kind() {
                format!("Address {} is already in use", &addr)
            } else {
                format!("Failed to listen on {}: {}", addr, err)
            };
            log::error!("{err_msg}");
            Err(err_msg)
        }
    }
}

pub(crate) fn create_server() -> Router {
    Router::new()
        .route("/", get(static_assets))
        .route("/*path", get(static_assets))
        .nest("/_live-server/*path", static_router())
        .route(
            "/live-server-ws",
            get(|ws: WebSocketUpgrade| async move {
                ws.on_failed_upgrade(|error| {
                    log::error!("Failed to upgrade websocket: {}", error);
                })
                .on_upgrade(|socket| async move {
                    let (mut sender, mut receiver) = socket.split();
                    let tx = TX.get().unwrap();
                    let mut rx = tx.subscribe();
                    let mut send_task = tokio::spawn(async move {
                        while rx.recv().await.is_ok() {
                            sender.send(Message::Text(String::new())).await.unwrap();
                        }
                    });
                    let mut recv_task =
                        tokio::spawn(
                            async move { while let Some(Ok(_)) = receiver.next().await {} },
                        );
                    tokio::select! {
                        _ = (&mut send_task) => recv_task.abort(),
                        _ = (&mut recv_task) => send_task.abort(),
                    };
                })
            }),
        )
}

async fn static_assets(req: Request<Body>) -> (StatusCode, HeaderMap, Body) {
    let addr = ADDR.get().unwrap();
    let root = ROOT.get().unwrap();

    // Get the path and mime of the static file.
    let mut path = req.uri().path().to_string();
    path.remove(0);

    let path = root.join(path);

    if !path.starts_with(root) {
        return internal_err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            "Path is outside of root directory",
        ));
    }

    let path = if path.is_dir() {
        let index = path.join("index.html");
        if tokio::fs::try_exists(&index).await.unwrap_or(false) {
            index
        } else {
            return serve_directory_listing(path).await;
        }
    } else {
        path
    };

    log::debug!("Serving {path:?}");

    let mime = mime_guess::from_path(&path).first_or_text_plain();
    let mut headers = HeaderMap::new();
    headers.append(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap(),
    );

    // Read the file.
    let file = match fs::read(&path) {
        Ok(file) => file,
        Err(err) => {
            match path.to_str() {
                Some(path) => log::warn!("Failed to read \"{}\": {}", path, err),
                None => log::warn!("Failed to read file with invalid path: {}", err),
            }
            let status_code = match err.kind() {
                ErrorKind::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            if mime == "text/html" {
                let script = format!(include_str!("templates/websocket.html"), addr);
                let html = format!(include_str!("templates/error.html"), script, err);
                let body = Body::from(html);

                return (status_code, headers, body);
            }
            return (status_code, headers, Body::empty());
        }
    };

    // Construct the response.
    let body = if mime == "text/html" && *WATCH.get().unwrap() {
        let text = match String::from_utf8(file) {
            Ok(text) => text,
            Err(err) => return internal_err(err),
        };

        let script = format!(include_str!("templates/websocket.html"), addr);

        Body::from(format!("{text}{script}"))
    } else {
        Body::from(file)
    };

    (StatusCode::OK, headers, body)
}

fn static_router() -> Router {
    Router::new()
        .route("/index.css", get(|r| asset(r, get_index_css)))
        .route("/entry.html", get(|r| asset(r, get_entry_html)))
        .route("/listing.html", get(|r| asset(r, get_listing_html)))
        .route("/dir.svg", get(|r| asset(r, get_dir_svg)))
        .route("/file.svg", get(|r| asset(r, get_file_svg)))
        .route("/dir-link.svg", get(|r| asset(r, get_dir_link_svg)))
        .route("/file-link.svg", get(|r| asset(r, get_file_link_svg)))
        .route("/unknown.svg", get(|r| asset(r, get_unknown_svg)))
}

async fn asset<F, Fut>(req: Request<Body>, content_fn: F) -> (StatusCode, HeaderMap, Body)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<String, std::io::Error>>,
{
    let path = Path::new(req.uri().path());

    let ext = path.extension().unwrap().to_string_lossy().to_string();

    let body = match content_fn().await {
        Ok(body) => body,
        Err(e) => return internal_err(e),
    };

    let mut headers = HeaderMap::new();
    headers.append(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime_type(&ext)).unwrap(),
    );

    (StatusCode::OK, headers, Body::from(body))
}

fn mime_type(ext: &str) -> &str {
    match ext {
        "css" => "text/css",
        "html" => "text/html",
        "svg" => "image/svg+xml",
        _ => "text/plain",
    }
}

pub fn internal_err<E: Error + Send + Sync + 'static>(err: E) -> (StatusCode, HeaderMap, Body) {
    log::error!("{}", err);

    let body = Body::from(err.to_string());
    let mut headers = HeaderMap::new();
    headers.append(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"));

    (StatusCode::INTERNAL_SERVER_ERROR, headers, body)
}
