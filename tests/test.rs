use live_server::listen;
use reqwest::StatusCode;

#[tokio::test]
async fn request() {
    let listener = listen("127.0.0.1:8000", "./tests/page", true)
        .await
        .unwrap();
    tokio::spawn(async {
        listener.start().await.unwrap();
    });

    // Test requesting index.html
    let response = reqwest::get("http://127.0.0.1:8000").await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get("content-type").unwrap();
    assert_eq!(content_type, "text/html");

    let text = response.text().await.unwrap().replace("\r\n", "\n");
    let target_text = format!(
        "{}{}",
        include_str!("./page/index.html"),
        format_args!(
            include_str!("../src/templates/websocket.html"),
            "127.0.0.1:8000"
        )
    )
    .replace("\r\n", "\n");
    assert_eq!(text, target_text);

    // Test requesting index.js
    let response = reqwest::get("http://127.0.0.1:8000/index.js")
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response.headers().get("content-type").unwrap();
    assert_eq!(content_type, "application/javascript");

    let text = response.text().await.unwrap().replace("\r\n", "\n");
    let target_text = include_str!("./page/index.js").replace("\r\n", "\n");
    assert_eq!(text, target_text);

    // Test requesting non-existent html file
    let response = reqwest::get("http://127.0.0.1:8000/404.html")
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let content_type = response.headers().get("content-type").unwrap();
    assert_eq!(content_type, "text/html");

    let text = response.text().await.unwrap();
    assert!(text.starts_with("<!DOCTYPE html>"));

    // Test requesting non-existent asset
    let response = reqwest::get("http://127.0.0.1:8000/favicon.ico")
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let content_type = response.headers().get("content-type").unwrap();
    assert_eq!(content_type, "image/x-icon");
}
