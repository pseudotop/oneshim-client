//! 정적 파일 임베드 및 서빙.
//!
//! rust-embed를 사용하여 React 빌드 결과를 바이너리에 임베드.

use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

/// 프론트엔드 빌드 결과 임베드
///
/// `frontend/dist` 디렉토리의 파일들을 바이너리에 포함
#[derive(Embed)]
#[folder = "frontend/dist"]
#[include = "*.html"]
#[include = "*.js"]
#[include = "*.css"]
#[include = "*.svg"]
#[include = "*.png"]
#[include = "*.ico"]
#[include = "*.json"]
#[include = "*.woff"]
#[include = "*.woff2"]
#[include = "assets/**/*"]
struct Assets;

/// 정적 파일 서빙을 위한 fallback 핸들러
pub async fn serve_static(uri: Uri) -> Response {
    serve_static_impl(uri)
}

/// 정적 파일 서빙 구현
fn serve_static_impl(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // 빈 경로는 index.html로
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();

            // Cache-Control 설정
            let cache_control = if path.ends_with(".html") {
                "no-cache"
            } else if path.contains("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "public, max-age=3600"
            };

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime.as_ref()),
                    (header::CACHE_CONTROL, cache_control),
                ],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => {
            // SPA 라우팅: 알 수 없는 경로는 index.html로
            if let Some(index) = Assets::get("index.html") {
                Html(String::from_utf8_lossy(&index.data).to_string()).into_response()
            } else {
                // 개발 중 프론트엔드 빌드 없을 때 안내
                (StatusCode::OK, Html(DEV_PLACEHOLDER.to_string())).into_response()
            }
        }
    }
}

/// 개발 중 프론트엔드 미빌드 시 표시할 페이지
const DEV_PLACEHOLDER: &str = r#"<!DOCTYPE html>
<html lang="ko">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>ONESHIM Dashboard</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);
            color: #e0e0e0;
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
        }
        .container {
            text-align: center;
            padding: 40px;
            max-width: 600px;
        }
        h1 {
            font-size: 2.5rem;
            margin-bottom: 1rem;
            background: linear-gradient(90deg, #00d9ff, #00ff88);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
        }
        .subtitle {
            color: #888;
            margin-bottom: 2rem;
        }
        .status {
            background: rgba(255,255,255,0.05);
            border-radius: 12px;
            padding: 24px;
            margin-bottom: 2rem;
        }
        .status h2 {
            color: #00d9ff;
            margin-bottom: 1rem;
        }
        .api-list {
            text-align: left;
            list-style: none;
        }
        .api-list li {
            padding: 8px 0;
            border-bottom: 1px solid rgba(255,255,255,0.1);
        }
        .api-list code {
            background: rgba(0,217,255,0.1);
            padding: 2px 8px;
            border-radius: 4px;
            font-family: 'SF Mono', monospace;
        }
        .build-hint {
            background: #2d2d44;
            padding: 16px;
            border-radius: 8px;
            font-family: 'SF Mono', monospace;
            font-size: 0.9rem;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>ONESHIM</h1>
        <p class="subtitle">로컬 웹 대시보드</p>

        <div class="status">
            <h2>✅ API 서버 실행 중</h2>
            <ul class="api-list">
                <li><code>GET /api/stats/summary</code> - 오늘 요약</li>
                <li><code>GET /api/metrics</code> - 시스템 메트릭</li>
                <li><code>GET /api/processes</code> - 프로세스 스냅샷</li>
                <li><code>GET /api/frames</code> - 스크린샷 목록</li>
                <li><code>GET /api/events</code> - 이벤트 로그</li>
                <li><code>GET /api/idle</code> - 유휴 기간</li>
                <li><code>GET /api/sessions</code> - 세션 목록</li>
            </ul>
        </div>

        <p style="margin-bottom: 1rem; color: #888;">프론트엔드 빌드:</p>
        <div class="build-hint">
            cd crates/oneshim-web/frontend<br>
            pnpm install && pnpm build
        </div>
    </div>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_placeholder_is_valid_html() {
        assert!(DEV_PLACEHOLDER.contains("<!DOCTYPE html>"));
        assert!(DEV_PLACEHOLDER.contains("ONESHIM"));
    }
}
