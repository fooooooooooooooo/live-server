use std::io::ErrorKind;

use async_std::{fs, path::Path};

macro_rules! embed_file {
    ($name:ident, $filename:expr) => {
        #[cfg(debug_assertions)]
        async fn $name() -> Result<String, std::io::Error> {
            fs::read_to_string(concat!("src/", $filename)).await
        }

        #[cfg(not(debug_assertions))]
        async fn $name() -> Result<String, std::io::Error> {
            Ok(include_str!($filename).to_owned())
        }
    };
}

embed_file!(get_index_css, "public/index.css");

embed_file!(get_entry_html, "public/entry.html");
embed_file!(get_listing_html, "public/listing.html");

embed_file!(get_dir_svg, "public/dir.svg");
embed_file!(get_file_svg, "public/file.svg");
embed_file!(get_dir_link_svg, "public/dir_link.svg");
embed_file!(get_file_link_svg, "public/file_link.svg");
embed_file!(get_unknown_svg, "public/unknown.svg");

pub async fn get_static_file<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    match path.as_ref().to_string_lossy().to_string().as_str() {
        "./public/index.css" => get_index_css().await,

        "./public/entry.html" => get_entry_html().await,
        "./public/listing.html" => get_listing_html().await,

        "./public/dir.svg" => get_dir_svg().await,
        "./public/file.svg" => get_file_svg().await,
        "./public/dir_link.svg" => get_dir_link_svg().await,
        "./public/file_link.svg" => get_file_link_svg().await,
        "./public/unknown.svg" => get_unknown_svg().await,

        _ => Err(std::io::Error::new(
            ErrorKind::NotFound,
            format!("could not read file `{}`", path.as_ref().to_string_lossy()),
        )),
    }
}
