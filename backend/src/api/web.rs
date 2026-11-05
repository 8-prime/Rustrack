use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::{Embed, EmbeddedFile};

/// The built frontend, compiled into the binary. See `build.rs` for how the
/// folder is chosen.
#[derive(Embed)]
#[folder = "$RUSTRACK_WEB_DIST"]
struct Assets;

/// Vite content-hashes everything under `assets/`, so a given URL never changes.
const HASHED_ASSET_PREFIX: &str = "assets/";

pub async fn handler(uri: Uri) -> Response {
    let path = match uri.path().trim_start_matches('/') {
        "" => "index.html",
        path => path,
    };

    if let Some(file) = Assets::get(path) {
        return serve(path, file);
    }

    let Some(index) = Assets::get("index.html") else {
        return (
            StatusCode::NOT_FOUND,
            "this build has no frontend embedded; run `pnpm build` in web/ and rebuild",
        )
            .into_response();
    };

    // A missing hashed asset means a stale page, and answering it with HTML
    // would only hide that. Any other path is a client-side route, which has to
    // survive a reload or a shared link.
    if path.starts_with(HASHED_ASSET_PREFIX) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    serve("index.html", index)
}

fn serve(path: &str, file: EmbeddedFile) -> Response {
    let mut response = (
        [(header::CONTENT_TYPE, file.metadata.mimetype().to_owned())],
        Body::from(file.data),
    )
        .into_response();

    if path.starts_with(HASHED_ASSET_PREFIX) {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}
