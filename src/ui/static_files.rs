use axum::{
    body::Body,
    http::{Response, StatusCode, header},
    response::IntoResponse,
};

// Embed static files at compile time
const CUSTOM_CSS: &[u8] = include_bytes!("../static/css/custom.css");
const LOGO_PNG: &[u8] = include_bytes!("../static/img/logo.png");
const FAVICON_ICO: &[u8] = include_bytes!("../static/img/favicon.ico");

pub async fn serve_static(path: axum::extract::Path<String>) -> impl IntoResponse {
    let path = path.0;

    match path.as_str() {
        "css/custom.css" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/css")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(CUSTOM_CSS))
            .unwrap(),
        "img/logo.png" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(LOGO_PNG))
            .unwrap(),
        "img/favicon.ico" => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/x-icon")
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(FAVICON_ICO))
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap(),
    }
}
