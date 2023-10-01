use async_std::{path::PathBuf, prelude::*};
use chrono::{DateTime, Local};
use std::time::SystemTime;
use tide::{Body, Response};

use crate::{server::internal_err, static_files::get_static_file};

pub async fn serve_directory_listing(dir: PathBuf) -> Result<Response, tide::Error> {
    let dir_name = dir.to_string_lossy().to_string();
    let dir_name = dir_name.trim_start_matches('.');

    let mut dir = dir.read_dir().await.map_err(internal_err)?;

    let mut entries = vec![];
    let mut rows = String::new();

    while let Some(entry) = dir.next().await {
        let entry = entry.map_err(internal_err)?;
        let entry_type = get_entry_type(&entry.path()).await;

        entries.push((entry, entry_type));
    }

    entries.sort_by(|(_, a), (_, b)| a.value().cmp(&b.value()));

    for (entry, entry_type) in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();

        let path = entry.path();
        let path = path.to_string_lossy();
        let path = path.trim_start_matches('.');

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

        let mut template = get_static_file("./public/entry.html").await.map_err(internal_err)?;

        template = render(template, "icon", entry_type.to_icon().await.map_err(internal_err)?);
        template = render(template, "path", path);
        template = render(template, "name", escape_html(name));
        template = render(template, "size", escape_html(bytes.unwrap_or_default()));
        template = render(template, "modified", escape_html(modified.unwrap_or_default()));

        rows.push_str(&template);
    }

    let mut template = get_static_file("./public/listing.html").await.map_err(internal_err)?;

    template = render(template, "directory", escape_html(dir_name));
    template = render(template, "entries", rows);

    let mut response: Response = Body::from_bytes(template.into()).into();

    response.set_content_type("text/html");

    Ok(response)
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
            EntryType::Dir => get_static_file("./public/dir.svg"),
            EntryType::File => get_static_file("./public/file.svg"),
            EntryType::DirLink => get_static_file("./public/dir_link.svg"),
            EntryType::FileLink => get_static_file("./public/file_link.svg"),
            EntryType::Other => get_static_file("./public/unknown.svg"),
        }
        .await
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

async fn get_entry_type(path: &PathBuf) -> EntryType {
    if let Ok(metadata) = async_std::fs::symlink_metadata(path).await {
        if metadata.file_type().is_symlink() {
            if let Ok(target_metadata) = async_std::fs::metadata(path).await {
                if target_metadata.is_dir() {
                    return EntryType::DirLink;
                } else if target_metadata.is_file() {
                    return EntryType::FileLink;
                }
            }
        } else if path.is_dir().await {
            return EntryType::Dir;
        } else if path.is_file().await {
            return EntryType::File;
        }
    }

    EntryType::Other
}
