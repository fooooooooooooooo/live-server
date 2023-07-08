//! Launch a local network server with live reload feature for static pages.
//!
//! ## Create live server
//! ```
//! use live_server::listen;
//! listen("127.0.0.1", 8080, "./").await.unwrap();
//! ```
//!
//! ## Enable logs (Optional)
//! ```rust
//! env_logger::init();
//! ```

mod server;
mod watcher;

use std::{collections::HashMap, sync::Arc};

use async_std::{path::PathBuf, sync::Mutex, task};

/// Watch the directory and create a static server.
/// ```
/// use live_server::listen;
/// listen("127.0.0.1", 8080, "./").await.unwrap();
/// ```
pub async fn listen<R: Into<PathBuf>>(
    host: &str,
    port: u16,
    root: R,
    watch: bool,
) -> Result<(), std::io::Error> {
    let connections = Arc::new(Mutex::new(HashMap::new()));
    let root: PathBuf = root.into();

    if watch {
        let connections = Arc::clone(&connections);
        let root = root.clone();

        task::spawn(async move { watcher::watch(root, &connections).await });
    }

    server::serve(host, port, root, connections, watch).await
}
