//! gRPC 클라이언트 통합 테스트
//!
//! Mock 서버(50052)와 연결하여 gRPC 서비스 테스트
//!
//! 실행:
//!   cargo run -p oneshim-network --example grpc_test --features grpc
//!
//! 산업 현장용 ASCII 출력 (이모지 미지원 환경):
//!   NO_EMOJI=1 cargo run -p oneshim-network --example grpc_test --features grpc

use std::collections::HashMap;
use std::sync::Arc;

use oneshim_network::auth::TokenManager;
use oneshim_network::grpc::{
    ContextBatchUploadRequest, FeedbackType, GrpcConfig, SuggestionType, UnifiedClient,
};

/// 산업 현장 호환 출력 헬퍼
struct Output {
    use_emoji: bool,
}

impl Output {
    fn new() -> Self {
        let use_emoji = std::env::var("NO_EMOJI").is_err();
        Self { use_emoji }
    }

    fn ok(&self) -> &'static str {
        if self.use_emoji {
            "✅"
        } else {
            "[OK]"
        }
    }

    fn err(&self) -> &'static str {
        if self.use_emoji {
            "❌"
        } else {
            "[ERR]"
        }
    }

    fn info(&self) -> &'static str {
        if self.use_emoji {
            "ℹ️"
        } else {
            "[INFO]"
        }
    }

    fn warn(&self) -> &'static str {
        if self.use_emoji {
            "⚠️"
        } else {
            "[WARN]"
        }
    }

    fn timeout(&self) -> &'static str {
        if self.use_emoji {
            "⏱️"
        } else {
            "[TIMEOUT]"
        }
    }

    fn msg(&self) -> &'static str {
        if self.use_emoji {
            "📨"
        } else {
            "[MSG]"
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 로깅 설정
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let out = Output::new();

    println!("============================================================");
    println!("Rust gRPC 클라이언트 테스트");
    println!("서버: localhost:50052");
    if !out.use_emoji {
        println!("모드: ASCII (NO_EMOJI=1)");
    }
    println!("============================================================");

    // gRPC 설정 (Mock 서버 50052 사용)
    let grpc_config = GrpcConfig {
        use_grpc_auth: true,
        use_grpc_context: true,
        grpc_endpoint: "http://127.0.0.1:50052".to_string(),
        grpc_fallback_ports: vec![50051, 50053], // fallback 포트 목록
        rest_endpoint: "http://127.0.0.1:8000".to_string(),
        connect_timeout_secs: 10,
        request_timeout_secs: 30,
        use_tls: false,
        mtls_enabled: false,
        tls_domain_name: None,
        tls_ca_cert_path: None,
        tls_client_cert_path: None,
        tls_client_key_path: None,
    };

    // TokenManager (REST fallback용)
    let token_manager = Arc::new(TokenManager::new("http://127.0.0.1:8000"));

    // UnifiedClient 생성
    let client = UnifiedClient::new(grpc_config, token_manager)?;

    // === 1. 로그인 테스트 ===
    println!("\n=== 1. 로그인 테스트 ===");
    match client
        .login(
            "admin@example.com",
            "test-password-placeholder",
            "test-org-001",
        )
        .await
    {
        Ok(response) => {
            println!("  {} 로그인 성공", out.ok());
            println!("  user_id: {:?}", response.user_id);
            println!(
                "  access_token: {}...",
                &response.access_token[..20.min(response.access_token.len())]
            );
        }
        Err(e) => {
            println!("  {} 로그인 실패: {}", out.err(), e);
        }
    }

    // === 2. 세션 생성 테스트 ===
    println!("\n=== 2. 세션 생성 테스트 ===");
    let device_info: HashMap<String, String> = [
        ("os".to_string(), "macOS".to_string()),
        ("version".to_string(), "0.1.3".to_string()),
    ]
    .into_iter()
    .collect();

    match client.create_session("rust-client-001", device_info).await {
        Ok(response) => {
            println!("  {} 세션 생성 성공", out.ok());
            println!("  session_id: {}", response.session_id);
            println!("  user_id: {}", response.user_id);

            // === 3. 하트비트 테스트 ===
            println!("\n=== 3. 하트비트 테스트 ===");
            match client
                .heartbeat(&response.session_id, "rust-client-001")
                .await
            {
                Ok(success) => {
                    println!("  {} 하트비트 성공: {}", out.ok(), success);
                }
                Err(e) => {
                    println!("  {} 하트비트 실패: {}", out.err(), e);
                }
            }

            // === 3.5. 배치 업로드 테스트 ===
            println!("\n=== 3.5. 배치 업로드 테스트 ===");
            let batch_request = ContextBatchUploadRequest {
                client_id: "rust-client-001".to_string(),
                session_id: response.session_id.clone(),
                upload_trigger: 1, // SCHEDULED
                upload_timestamp: None,
                events: vec![], // 빈 이벤트 테스트
                frames: vec![], // 빈 프레임 테스트
                client_stats: HashMap::new(),
                last_sync_timestamp: None,
                sync_sequence: 1,
            };
            match client.upload_batch(batch_request).await {
                Ok(batch_response) => {
                    println!("  {} 배치 업로드 성공", out.ok());
                    println!("    status: {}", batch_response.status);
                    println!("    processed_events: {}", batch_response.processed_events);
                    println!("    processed_frames: {}", batch_response.processed_frames);
                    println!("    next_sync_sequence: {}", batch_response.sync_sequence);
                }
                Err(e) => {
                    println!("  {} 배치 업로드 실패: {}", out.err(), e);
                }
            }

            // === 4. 제안 스트리밍 테스트 ===
            println!("\n=== 4. 제안 스트리밍 테스트 ===");
            match client
                .subscribe_suggestions(&response.session_id, "rust-client-001")
                .await
            {
                Ok(mut stream) => {
                    println!("  {} 제안 스트림 구독 성공", out.ok());
                    println!("  첫 번째 제안 대기 중... (5초 타임아웃)");

                    // 첫 번째 제안만 수신 후 종료 (타임아웃 적용)
                    let timeout_duration = std::time::Duration::from_secs(5);
                    match tokio::time::timeout(timeout_duration, stream.message()).await {
                        Ok(Ok(Some(suggestion))) => {
                            println!("  {} 제안 수신:", out.msg());
                            println!("    suggestion_id: {}", suggestion.suggestion_id);
                            println!("    content: {}", suggestion.content);
                            println!("    priority: {:?}", suggestion.priority);
                        }
                        Ok(Ok(None)) => {
                            println!("  {} 스트림 종료됨 (서버에서 종료)", out.info());
                        }
                        Ok(Err(e)) => {
                            println!("  {} 스트림 에러: {}", out.warn(), e);
                        }
                        Err(_) => {
                            println!("  {} 타임아웃 (5초 내 제안 없음 - 정상)", out.timeout());
                        }
                    }
                }
                Err(e) => {
                    println!("  {} 제안 스트림 구독 실패: {}", out.err(), e);
                }
            }

            // === 5. 피드백 전송 테스트 ===
            println!("\n=== 5. 피드백 전송 테스트 ===");
            match client
                .send_feedback(
                    "test-suggestion-001",
                    FeedbackType::Accepted,
                    Some("테스트 피드백입니다"),
                )
                .await
            {
                Ok(()) => {
                    println!("  {} 피드백 전송 성공", out.ok());
                }
                Err(e) => {
                    println!("  {} 피드백 전송 실패: {}", out.err(), e);
                }
            }

            // === 6. 제안 목록 조회 테스트 ===
            println!("\n=== 6. 제안 목록 조회 테스트 ===");
            match client.list_suggestions(vec![], 10).await {
                Ok(response) => {
                    println!("  {} 제안 목록 조회 성공", out.ok());
                    println!("    조회된 제안 수: {}", response.suggestions.len());
                    for suggestion in response.suggestions.iter().take(3) {
                        println!("    - {}: {}", suggestion.suggestion_id, suggestion.content);
                    }
                }
                Err(e) => {
                    println!("  {} 제안 목록 조회 실패: {}", out.err(), e);
                }
            }

            // 특정 유형 필터 테스트
            println!("\n=== 6.1. 특정 유형 제안 조회 테스트 ===");
            match client
                .list_suggestions(vec![SuggestionType::WorkGuidance], 5)
                .await
            {
                Ok(response) => {
                    println!(
                        "  {} WorkGuidance 제안 조회 성공: {} 개",
                        out.ok(),
                        response.suggestions.len()
                    );
                }
                Err(e) => {
                    println!("  {} 특정 유형 제안 조회 실패: {}", out.err(), e);
                }
            }
        }
        Err(e) => {
            println!("  {} 세션 생성 실패: {}", out.err(), e);
        }
    }

    println!("\n============================================================");
    println!("테스트 완료");
    println!("============================================================");

    Ok(())
}
