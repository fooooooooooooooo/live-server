use async_std::{
    fs,
    path::{Path, PathBuf},
    prelude::*,
    sync::Mutex,
};
use std::sync::Arc;
use std::{collections::HashMap, error::Error};
use tide::{listener::Listener, Body, Request, Response, StatusCode};
use tide_websockets::{WebSocket, WebSocketConnection};
use uuid::Uuid;

use crate::{listing::serve_directory_listing, static_files::get_static_file};

macro_rules! static_assets_service {
    ($app: expr, $route: expr, $host: ident, $port: ident, $root: ident, $watch: ident) => {
        let host_clone = $host.to_string();
        let port_clone = $port;
        let root_clone = $root.clone();
        let watch_clone = $watch;
        $app.at($route).get(move |req: Request<()>| {
            let host = host_clone.clone();
            let port = port_clone.clone();
            let root = root_clone.clone();
            let watch = watch_clone.clone();
            static_assets(req, host, port, root, watch)
        });
    };
}

pub async fn serve(
    host: &str,
    port: u16,
    root: PathBuf,
    connections: Arc<Mutex<HashMap<Uuid, WebSocketConnection>>>,
    watch: bool,
) -> Result<(), std::io::Error> {
    let mut listener = create_listener(host, port, &root, connections, watch).await;
    listener.accept().await
}

async fn create_listener(
    host: &str,
    port: u16,
    root: &PathBuf,
    connections: Arc<Mutex<HashMap<Uuid, WebSocketConnection>>>,
    watch: bool,
) -> impl Listener<()> {
    let mut port = port;
    // Loop until the port is available
    loop {
        let app = create_server(host, port, root, Arc::clone(&connections), watch);
        match app.bind(format!("{host}:{port}")).await {
            Ok(listener) => {
                log::info!("Listening on http://{}:{}/", host, port);
                break listener;
            }
            Err(err) => {
                if let std::io::ErrorKind::AddrInUse = err.kind() {
                    log::warn!("Port {} is already in use", port);
                    port += 1;
                } else {
                    log::error!("Failed to listen on {}:{}: {}", host, port, err);
                }
            }
        }
    }
}

fn create_server(
    host: &str,
    port: u16,
    root: &PathBuf,
    connections: Arc<Mutex<HashMap<Uuid, WebSocketConnection>>>,
    watch: bool,
) -> tide::Server<()> {
    let mut app = tide::new();

    static_assets_service!(app, "/", host, port, root, watch);
    static_assets_service!(app, "/*", host, port, root, watch);

    app.at("/_live-server/*").get(public_dir);

    app.at("/live-server-ws")
        .get(WebSocket::new(move |_request, mut stream| {
            let connections = Arc::clone(&connections);
            async move {
                let uuid = Uuid::new_v4();

                // Add the connection to clients when opening a new connection
                connections.lock().await.insert(uuid, stream.clone());

                // Waiting for the connection to be closed
                while let Some(Ok(_)) = stream.next().await {}

                // Remove the connection from clients when it is closed
                connections.lock().await.remove(&uuid);

                Ok(())
            }
        }));
    app
}

async fn static_assets(
    req: Request<()>,
    host: String,
    port: u16,
    root: PathBuf,
    watch: bool,
) -> Result<Response, tide::Error> {
    // Get the path and mime of the static file.
    let mut path = req.url().path().to_string();

    path.remove(0);

    let mut path = root.join(path);

    if path.is_dir().await {
        let dir = path.clone();
        path.push("index.html");

        if !path.exists().await {
            return serve_directory_listing(dir).await;
        }
    }

    let mime = mime_guess::from_path(&path).first_or_text_plain();

    // Read the file.
    let mut file = fs::read(&path).await.map_err(not_found)?;

    // Construct the response.
    if mime == "text/html" {
        let text = String::from_utf8(file).map_err(internal_err)?;

        if watch {
            let script = format!(include_str!("scripts/websocket.js"), host, port);
            file = format!("{text}<script>{script}</script>").into_bytes();
        } else {
            file = text.into_bytes();
        }
    }
    let mut response: Response = Body::from_bytes(file).into();
    response.set_content_type(mime.to_string().as_str());

    Ok(response)
}

async fn public_dir(req: Request<()>) -> Result<Response, tide::Error> {
    let path = Path::new(req.url().path());

    let path = path.strip_prefix("/_live-server").map_err(internal_err)?;

    let path = Path::new("./public").join(path);
    let ext = path.extension().unwrap().to_string_lossy().to_string();

    let mut response: Response = Body::from_bytes(get_static_file(path).await.map_err(not_found)?.into()).into();
    response.set_content_type(mime_type(&ext));

    Ok(response)
}

fn mime_type(ext: &str) -> &str {
    match ext {
        "css" => "text/css",
        "html" => "text/html",
        "svg" => "image/svg+xml",
        _ => "text/plain",
    }
}

pub fn internal_err<E: Error + Send + Sync + 'static>(err: E) -> tide::Error {
    log::error!("{}", err);

    tide::Error::from_str(StatusCode::InternalServerError, err)
}

pub fn not_found<E: Error + Send + Sync + 'static>(err: E) -> tide::Error {
    log::warn!("{}", err);

    tide::Error::from_str(StatusCode::NotFound, err)
}
