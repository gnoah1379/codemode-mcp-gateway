use axum::{
    extract::OriginalUri,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "public/"]
struct WebAssets;

pub async fn serve(OriginalUri(uri): OriginalUri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let asset_path = if path.is_empty() || (!path.contains('.') && WebAssets::get(path).is_none()) {
        "index.html"
    } else {
        path
    };
    let asset = WebAssets::get(asset_path);
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(asset_path).first_or_octet_stream();
    (
        [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
        asset.data.into_owned(),
    )
        .into_response()
}
