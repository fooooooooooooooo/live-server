use axum::http::{header, HeaderValue, StatusCode};
use axum::{body::Body, http::HeaderMap};
use chrono::{DateTime, Local};
use std::path::{Path, StripPrefixError};
use std::{path::PathBuf, time::SystemTime};
use tokio::fs::DirEntry;

use crate::path_to_string_but_readable;
use crate::server::internal_err;
use crate::static_files::{
    get_dir_link_svg, get_dir_svg, get_entry_html, get_file_link_svg, get_file_svg,
    get_listing_html, get_unknown_svg,
};

pub async fn serve_directory_listing(root: &Path, dir: PathBuf) -> (StatusCode, HeaderMap, Body) {
    let dir_string = path_to_string_but_readable(&dir);

    let mut headers = HeaderMap::new();
    headers.append(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );

    let mut dir = match tokio::fs::read_dir(&dir).await {
        Ok(dir) => dir,
        Err(e) => return internal_err(e),
    };

    let mut entries = vec![];
    let mut rows = String::new();

    while let Some(entry) = match dir.next_entry().await {
        Ok(entry) => entry,
        Err(e) => return internal_err(e),
    } {
        let entry_type = get_entry_type(&entry.path()).await;

        entries.push((entry, entry_type));
    }

    entries.sort_by(|(_, a), (_, b)| a.value().cmp(&b.value()));

    for (entry, entry_type) in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();

        let path = match entry_to_path(&entry, root) {
            Ok(entry) => entry,
            Err(e) => return internal_err(e),
        };

        let (bytes, modified) = match entry.metadata().await {
            Ok(metadata) => (
                if !entry_type.is_dir() {
                    Some(format_file_size(metadata.len()))
                } else {
                    None
                },
                metadata.modified().ok().and_then(format_system_time),
            ),
            _ => (None, None),
        };

        let mut template = match get_entry_html().await {
            Ok(template) => template,
            Err(e) => return internal_err(e),
        };

        template = render(
            template,
            "icon",
            match entry_type.to_icon().await {
                Ok(icon) => icon,
                Err(e) => return internal_err(e),
            },
        );

        template = render(template, "path", path);
        template = render(template, "name", escape_html(name));
        template = render(template, "size", escape_html(bytes.unwrap_or_default()));
        template = render(
            template,
            "modified",
            escape_html(modified.unwrap_or_default()),
        );

        rows.push_str(&template);
    }

    let mut template = match get_listing_html().await {
        Ok(template) => template,
        Err(e) => return internal_err(e),
    };

    template = render(template, "directory", escape_html(dir_string));
    template = render(template, "entries", rows);

    let body = Body::from(template);

    (StatusCode::OK, headers, body)
}

fn entry_to_path(entry: &DirEntry, root: &Path) -> Result<String, StripPrefixError> {
    let path = entry.path();

    let path = if let Ok(p) = path.strip_prefix(root) {
        p.to_path_buf()
    } else {
        path
    };

    Ok(format!("/{}", path_to_string_but_readable(path)))
}

fn render<S: AsRef<str>>(template: String, var_name: &str, value: S) -> String {
    template.replace(&format!("{{{{ {} }}}}", var_name), value.as_ref())
}

fn format_system_time(system_time: SystemTime) -> Option<String> {
    let dt: DateTime<Local> = DateTime::from(system_time);

    Some(dt.format("%b %-e %Y %H:%M:%S").to_string())
}

fn format_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }

    const UNITS: [&str; 9] = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    const K: u32 = 1024;

    let exp = (bytes as f64).log(K as f64).floor() as usize;
    let size = bytes as f64 / K.pow(exp as u32) as f64;

    format!("{} {}", format_float(size), UNITS[exp])
}

fn format_float(value: f64) -> String {
    format!("{:.2}", value)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn escape_html<S: AsRef<str>>(input: S) -> String {
    input
        .as_ref()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Clone)]
enum EntryType {
    Dir,
    File,
    DirLink,
    FileLink,
    Other,
}

impl EntryType {
    fn is_dir(&self) -> bool {
        matches!(self, EntryType::Dir)
    }

    const fn value(&self) -> u8 {
        match self {
            EntryType::Dir => 0,
            EntryType::DirLink => 1,
            EntryType::File => 2,
            EntryType::FileLink => 3,
            EntryType::Other => 4,
        }
    }

    async fn to_icon(&self) -> Result<String, std::io::Error> {
        match self {
            EntryType::Dir => get_dir_svg().await,
            EntryType::File => get_file_svg().await,
            EntryType::DirLink => get_dir_link_svg().await,
            EntryType::FileLink => get_file_link_svg().await,
            EntryType::Other => get_unknown_svg().await,
        }
    }
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            EntryType::Dir => "dir",
            EntryType::File => "file",
            EntryType::DirLink => "dir-link",
            EntryType::FileLink => "file-link",
            EntryType::Other => "unknown",
        })
    }
}

async fn get_entry_type<P: AsRef<Path>>(path: P) -> EntryType {
    let path = path.as_ref();

    if let Ok(metadata) = tokio::fs::symlink_metadata(path).await {
        if metadata.file_type().is_symlink() {
            if let Ok(target_metadata) = tokio::fs::metadata(path).await {
                if target_metadata.is_dir() {
                    return EntryType::DirLink;
                } else if target_metadata.is_file() {
                    return EntryType::FileLink;
                }
            }
        } else if metadata.is_dir() {
            return EntryType::Dir;
        } else if metadata.is_file() {
            return EntryType::File;
        }
    }

    EntryType::Other
}
