macro_rules! embed_file {
    ($name:ident, $filename:expr) => {
        #[cfg(debug_assertions)]
        pub async fn $name() -> Result<String, std::io::Error> {
            tokio::fs::read_to_string(concat!("src/", $filename)).await
        }

        #[cfg(not(debug_assertions))]
        pub async fn $name() -> Result<String, std::io::Error> {
            Ok(include_str!($filename).to_owned())
        }
    };
}

embed_file!(get_index_css, "public/index.css");

embed_file!(get_entry_html, "templates/entry.html");
embed_file!(get_listing_html, "templates/listing.html");

embed_file!(get_dir_svg, "public/dir.svg");
embed_file!(get_file_svg, "public/file.svg");
embed_file!(get_dir_link_svg, "public/dir_link.svg");
embed_file!(get_file_link_svg, "public/file_link.svg");
embed_file!(get_unknown_svg, "public/unknown.svg");
