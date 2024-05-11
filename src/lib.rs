//! Launch a local network server with live reload feature for static pages.
//!
//! ## Create live server
//! ```
//! use live_server::listen;
//!
//! async fn serve() -> Result<(), Box<dyn std::error::Error>> {
//!     listen("127.0.0.1:8080", "./").await?.start().await
//! }
//! ```
//!
//! ## Enable logs (Optional)
//! ```rust
//! env_logger::init();
//! ```

mod listing;
mod server;
mod static_files;
mod watcher;

use std::{error::Error, net::IpAddr, path::PathBuf};

use axum::Router;
use local_ip_address::local_ip;
use server::{create_listener, create_server};
use tokio::{
    net::TcpListener,
    sync::{broadcast, OnceCell},
};
use watcher::{create_watcher, Watcher};

static WATCH: OnceCell<bool> = OnceCell::const_new();
static ADDR: OnceCell<String> = OnceCell::const_new();
static ROOT: OnceCell<PathBuf> = OnceCell::const_new();
static TX: OnceCell<broadcast::Sender<()>> = OnceCell::const_new();

pub struct Listener {
    tcp_listener: TcpListener,
    router: Router,
    root_path: PathBuf,
    watcher: Option<Watcher>,
}

impl Listener {
    /// Start live-server.
    ///
    /// ```
    /// use live_server::listen;
    ///
    /// async fn serve() -> Result<(), Box<dyn std::error::Error>> {
    ///     listen("127.0.0.1:8080", "./").await?.start().await
    /// }
    /// ```
    pub async fn start(self) -> Result<(), Box<dyn Error>> {
        ROOT.set(self.root_path.clone())?;
        let (tx, _) = broadcast::channel(16);
        TX.set(tx)?;

        let server_future = tokio::spawn(server::serve(self.tcp_listener, self.router));

        if let Some(watcher) = self.watcher {
            let watcher_future = tokio::spawn(watcher::watch(self.root_path, watcher));
            tokio::try_join!(watcher_future, server_future)?;
        } else {
            tokio::try_join!(server_future)?;
        }

        Ok(())
    }

    /// Return the link of the server, like `http://127.0.0.1:8080`.
    ///
    /// ```
    /// use live_server::listen;
    ///
    /// async fn serve() {
    ///     let listener = listen("127.0.0.1:8080", "./").await.unwrap();
    ///     let link = listener.link().unwrap();
    ///     assert_eq!(link, "http://127.0.0.1:8080");
    /// }
    /// ```
    ///
    /// This is useful when you did not specify the host or port (e.g. `listen("0.0.0.0:0", ".")`),
    /// because this method will return the specific address.
    pub fn link(&self) -> Result<String, Box<dyn Error>> {
        let addr = self.tcp_listener.local_addr()?;
        let port = addr.port();
        let host = addr.ip();
        let host = match host.is_unspecified() {
            true => local_ip()?,
            false => host,
        };

        Ok(match host {
            IpAddr::V4(host) => format!("http://{host}:{port}"),
            IpAddr::V6(host) => format!("http://[{host}]:{port}"),
        })
    }
}

/// Create live-server listener
///
/// ```
/// use live_server::listen;
///
/// async fn serve() -> Result<(), Box<dyn std::error::Error>> {
///     listen("127.0.0.1:8080", "./", true).await?.start().await
/// }
/// ```
pub async fn listen<A: Into<String>, R: Into<PathBuf>>(
    addr: A,
    root: R,
    watch: bool,
) -> Result<Listener, String> {
    WATCH.set(watch).unwrap();

    let tcp_listener = create_listener(addr.into()).await?;
    let router = create_server();

    let root = root.into();

    let root_path = match tokio::fs::canonicalize(&root).await {
        Ok(path) => path,
        Err(err) => {
            let err_msg = format!("Failed to get absolute path of {:?}: {}", root, err);
            log::error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    match root_path.clone().into_os_string().into_string() {
        Ok(path_str) => {
            log::info!("Listening on {}", path_str);
        }
        Err(_) => {
            let err_msg = format!("Failed to parse path to string for `{:?}`", root_path);
            log::error!("{}", err_msg);
            return Err(err_msg);
        }
    };

    let watcher = if watch {
        Some(create_watcher().await?)
    } else {
        None
    };

    Ok(Listener {
        tcp_listener,
        router,
        root_path,
        watcher,
    })
}
