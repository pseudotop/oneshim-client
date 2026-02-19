//! 빌드 스크립트 — 프론트엔드 빌드 상태 확인
//!
//! dist 폴더가 없거나 비어있으면 빌드 방법을 안내합니다.

use std::path::Path;

fn main() {
    // cargo:rerun-if-changed로 프론트엔드 소스 변경 감지
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/package.json");
    println!("cargo:rerun-if-changed=frontend/dist");

    let dist_path = Path::new("frontend/dist");
    let index_path = dist_path.join("index.html");

    // dist 폴더 또는 index.html이 없으면 경고
    if !dist_path.exists() || !index_path.exists() {
        println!("cargo:warning=================================================================================");
        println!("cargo:warning=  프론트엔드 빌드 필요!");
        println!("cargo:warning=  cd crates/oneshim-web/frontend && pnpm install && pnpm build");
        println!("cargo:warning=================================================================================");
    }
}
