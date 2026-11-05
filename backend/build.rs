//! Picks the directory `api::web` embeds. A built frontend isn't required to
//! compile the backend: without one the server simply has no UI, so backend
//! work doesn't depend on the Node and wasm toolchains.

use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let dist = manifest_dir.join("../web/dist");

    // Also covers the frontend being built after this script last ran: cargo
    // treats a missing path as changed.
    println!("cargo:rerun-if-changed={}", dist.display());

    let folder = if dist.join("index.html").is_file() {
        dist
    } else {
        println!("cargo:warning=web/dist not found; building without the frontend");
        let empty = PathBuf::from(env::var("OUT_DIR").unwrap()).join("no-frontend");
        std::fs::create_dir_all(&empty).unwrap();
        empty
    };

    println!("cargo:rustc-env=RUSTRACK_WEB_DIST={}", folder.display());
}
