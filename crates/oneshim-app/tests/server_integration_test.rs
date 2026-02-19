//! 서버 통합 테스트
//!
//! Mock 서버와 실제 클라이언트 코드를 연결하여 통합 테스트합니다.
//!
//! 실행:
//! ```
//! cargo test -p oneshim-app --test server_integration_test -- --nocapture
//! ```

mod mock_server;

use mock_server::MockServer;
use oneshim_core::models::event::{Event, EventBatch, UserEvent, UserEventType};
use oneshim_core::models::frame::{ContextUpload, FrameMetadata};
use oneshim_core::ports::api_client::ApiClient;
use oneshim_network::auth::TokenManager;
use oneshim_network::http_client::HttpApiClient;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// TokenManager 로그인 테스트
#[tokio::test]
async fn test_token_manager_login() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    let token_manager = TokenManager::new(server.url());

    // 로그인 시도
    let result = token_manager
        .login("test@example.com", "test-password-placeholder")
        .await;
    assert!(result.is_ok(), "로그인 실패: {:?}", result.err());

    // 토큰 획득 확인
    let token = token_manager.get_token().await;
    assert!(token.is_ok(), "토큰 획득 실패: {:?}", token.err());

    let token_str = token.unwrap();
    assert!(
        token_str.starts_with("mock_access_"),
        "토큰 형식 불일치: {}",
        token_str
    );

    println!("✅ 로그인 성공, 토큰: {}...", &token_str[..20]);
}

/// HttpApiClient 세션 생성 테스트
#[tokio::test]
async fn test_api_client_create_session() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    // HttpApiClient 생성
    let api_client =
        HttpApiClient::new(server.url(), token_manager, Duration::from_secs(30)).unwrap();

    // 세션 생성
    let client_id = format!("client_{}", Uuid::new_v4());
    let result = api_client.create_session(&client_id).await;

    assert!(result.is_ok(), "세션 생성 실패: {:?}", result.err());

    let session = result.unwrap();
    assert!(session.session_id.starts_with("session_"));
    assert_eq!(session.client_id, client_id);

    println!("✅ 세션 생성 성공: {}", session.session_id);
    println!("   - User ID: {}", session.user_id);
    println!("   - Capabilities: {:?}", session.capabilities);
}

/// 컨텍스트 업로드 테스트
#[tokio::test]
async fn test_api_client_upload_context() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    // HttpApiClient 생성
    let api_client =
        HttpApiClient::new(server.url(), token_manager, Duration::from_secs(30)).unwrap();

    // 컨텍스트 업로드 생성
    let context = ContextUpload {
        session_id: "test_session_123".to_string(),
        timestamp: chrono::Utc::now(),
        metadata: FrameMetadata {
            timestamp: chrono::Utc::now(),
            trigger_type: "AppSwitch".to_string(),
            app_name: "Visual Studio Code".to_string(),
            window_title: "server_integration_test.rs - oneshim-client".to_string(),
            resolution: (1920, 1080),
            importance: 0.8,
        },
        ocr_text: Some("테스트 텍스트".to_string()),
        image: None,
    };

    // 업로드
    let result = api_client.upload_context(&context).await;
    assert!(result.is_ok(), "컨텍스트 업로드 실패: {:?}", result.err());

    // 서버에 저장 확인
    assert_eq!(server.context_count(), 1);

    println!("✅ 컨텍스트 업로드 성공");
    println!("   - App: {}", context.metadata.app_name);
    println!("   - Window: {}", context.metadata.window_title);
}

/// 배치 이벤트 업로드 테스트
#[tokio::test]
async fn test_api_client_upload_batch() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    // HttpApiClient 생성
    let api_client =
        HttpApiClient::new(server.url(), token_manager, Duration::from_secs(30)).unwrap();

    // 이벤트 배치 생성
    let events: Vec<Event> = (0..5)
        .map(|i| {
            Event::User(UserEvent {
                event_id: Uuid::new_v4(),
                event_type: UserEventType::WindowChange,
                timestamp: chrono::Utc::now(),
                app_name: format!("App{}", i),
                window_title: format!("Window {}", i),
            })
        })
        .collect();

    let batch = EventBatch {
        session_id: "test_session_batch".to_string(),
        events,
        created_at: chrono::Utc::now(),
    };

    // 업로드
    let result = api_client.upload_batch(&batch).await;
    assert!(result.is_ok(), "배치 업로드 실패: {:?}", result.err());

    println!("✅ 배치 이벤트 업로드 성공");
    println!("   - Session ID: {}", batch.session_id);
    println!("   - Events: {} 개", batch.events.len());
}

/// 헬스체크 테스트
#[tokio::test]
async fn test_api_client_health_check() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    // HttpApiClient 생성
    let api_client =
        HttpApiClient::new(server.url(), token_manager, Duration::from_secs(30)).unwrap();

    // 세션 생성
    let session = api_client
        .create_session("test_client")
        .await
        .expect("세션 생성 실패");

    // 헬스체크
    let result = api_client.send_heartbeat(&session.session_id).await;
    assert!(result.is_ok(), "헬스체크 실패: {:?}", result.err());

    println!("✅ 헬스체크 성공");
}

/// 토큰 갱신 테스트
#[tokio::test]
async fn test_token_refresh() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    let token_manager = TokenManager::new(server.url());

    // 로그인
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    let token1 = token_manager.get_token().await.expect("토큰 획득 실패");

    // 강제 갱신
    token_manager.refresh().await.expect("토큰 갱신 실패");

    let token2 = token_manager.get_token().await.expect("토큰 획득 실패");

    // 토큰이 변경되었는지 확인
    assert_ne!(token1, token2, "토큰이 갱신되지 않음");

    println!("✅ 토큰 갱신 성공");
    println!("   - Old: {}...", &token1[..20]);
    println!("   - New: {}...", &token2[..20]);
}

/// 다중 요청 시퀀스 테스트
#[tokio::test]
async fn test_full_client_workflow() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // 1. TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("user@example.com", "test-password-placeholder")
        .await
        .expect("Step 1: 로그인 실패");
    println!("1️⃣ 로그인 완료");

    // 2. API 클라이언트 생성
    let api_client =
        HttpApiClient::new(server.url(), token_manager.clone(), Duration::from_secs(30)).unwrap();

    // 3. 세션 생성
    let session = api_client
        .create_session("oneshim_client_v1")
        .await
        .expect("Step 3: 세션 생성 실패");
    println!("2️⃣ 세션 생성: {}", session.session_id);

    // 4. 컨텍스트 업로드 (3회)
    for i in 0..3 {
        let context = ContextUpload {
            session_id: session.session_id.clone(),
            timestamp: chrono::Utc::now(),
            metadata: FrameMetadata {
                timestamp: chrono::Utc::now(),
                trigger_type: "Timer".to_string(),
                app_name: format!("App{}", i),
                window_title: format!("Document {}", i),
                resolution: (1920, 1080),
                importance: 0.5,
            },
            ocr_text: None,
            image: None,
        };
        api_client
            .upload_context(&context)
            .await
            .unwrap_or_else(|_| panic!("Step 4-{}: 컨텍스트 업로드 실패", i));
    }
    println!("3️⃣ 컨텍스트 업로드 3회 완료");

    // 5. 이벤트 배치 업로드
    let events: Vec<Event> = (0..10)
        .map(|i| {
            Event::User(UserEvent {
                event_id: Uuid::new_v4(),
                event_type: UserEventType::WindowChange,
                timestamp: chrono::Utc::now(),
                app_name: format!("App{}", i % 3),
                window_title: format!("Window {}", i),
            })
        })
        .collect();

    let batch = EventBatch {
        session_id: session.session_id.clone(),
        events,
        created_at: chrono::Utc::now(),
    };

    api_client
        .upload_batch(&batch)
        .await
        .expect("Step 5: 배치 업로드 실패");
    println!("4️⃣ 이벤트 배치 업로드 완료 (10개)");

    // 6. 헬스체크
    api_client
        .send_heartbeat(&session.session_id)
        .await
        .expect("Step 6: 헬스체크 실패");
    println!("5️⃣ 헬스체크 완료");

    // 결과 검증
    assert!(server.request_count() >= 7, "요청 수 부족");
    assert_eq!(server.context_count(), 3, "컨텍스트 수 불일치");
    assert_eq!(server.session_count(), 1, "세션 수 불일치");

    println!("\n✅ 전체 워크플로우 성공!");
    println!("   - 총 요청 수: {}", server.request_count());
    println!("   - 컨텍스트: {} 개", server.context_count());
    println!("   - 세션: {} 개", server.session_count());
}

/// 잘못된 인증 정보 테스트
#[tokio::test]
async fn test_invalid_credentials() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    let token_manager = TokenManager::new(server.url());

    // 로그인 시도 - 빈 credentials는 실패
    let result = token_manager.login("", "").await;

    // Mock 서버는 빈 credentials에 대해 401 반환
    assert!(result.is_err(), "빈 인증 정보로 로그인 성공하면 안됨");
    println!("✅ 잘못된 인증 정보 거부 확인");
}

/// 동시 요청 테스트
#[tokio::test]
async fn test_concurrent_requests() {
    let server = MockServer::start().await;
    println!("Mock 서버 시작: {}", server.url());

    // TokenManager 설정 및 로그인
    let token_manager = Arc::new(TokenManager::new(server.url()));
    token_manager
        .login("test@example.com", "test-password-placeholder")
        .await
        .expect("로그인 실패");

    // HttpApiClient 생성
    let api_client =
        Arc::new(HttpApiClient::new(server.url(), token_manager, Duration::from_secs(30)).unwrap());

    // 세션 생성
    let session = api_client
        .create_session("concurrent_test")
        .await
        .expect("세션 생성 실패");
    let session_id = session.session_id.clone();

    // 10개 동시 헬스체크 요청
    let mut handles = Vec::new();
    for _ in 0..10 {
        let client = api_client.clone();
        let sid = session_id.clone();
        handles.push(tokio::spawn(
            async move { client.send_heartbeat(&sid).await },
        ));
    }

    // 모든 요청 완료 대기
    let results: Vec<_> = futures::future::join_all(handles).await;

    // 모든 요청 성공 확인
    let success_count = results
        .iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
        .count();
    assert_eq!(success_count, 10, "일부 동시 요청 실패");

    println!("✅ 동시 요청 10개 모두 성공");
    println!("   - 총 요청 수: {}", server.request_count());
}
