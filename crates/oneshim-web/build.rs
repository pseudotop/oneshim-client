//!

use std::path::Path;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(oneshim_web_embedded_dist)");
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/dist");

    let dist_path = Path::new("frontend/dist");
    let index_path = dist_path.join("index.html");

    if !dist_path.exists() || !index_path.exists() {
        println!("cargo:warning=================================================================================");
        println!("cargo:warning= required!");
        println!("cargo:warning=  cd crates/oneshim-web/frontend && pnpm install && pnpm build");
        println!("cargo:warning=================================================================================");
    } else {
        println!("cargo:rustc-cfg=oneshim_web_embedded_dist");
    }
}
