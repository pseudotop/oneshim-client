# oneshim-suggestion

AI 제안 수신, 처리, 피드백을 담당하는 크레이트.

## 역할

- **제안 수신**: SSE 스트림에서 제안 이벤트 수신
- **우선순위 관리**: 중요도 기반 제안 정렬
- **피드백 전송**: 사용자 반응(수락/거절) 서버 전송
- **이력 관리**: 제안 히스토리 캐시

## 디렉토리 구조

```
oneshim-suggestion/src/
├── lib.rs        # 크레이트 루트
├── receiver.rs   # SuggestionReceiver - SSE 이벤트 → 제안 변환
├── queue.rs      # PriorityQueue - 우선순위 큐
├── feedback.rs   # FeedbackSender - 피드백 전송
├── presenter.rs  # SuggestionPresenter - UI 데이터 변환
└── history.rs    # SuggestionHistory - 이력 캐시
```

## 주요 컴포넌트

### SuggestionReceiver (receiver.rs)

SSE 이벤트를 제안으로 변환:

```rust
pub struct SuggestionReceiver {
    sse_client: Arc<dyn SseClient>,
    queue: Arc<PriorityQueue>,
    notifier: Arc<dyn DesktopNotifier>,
}

impl SuggestionReceiver {
    pub async fn start(&self, session_id: &str) -> Result<(), CoreError> {
        let (tx, mut rx) = mpsc::channel::<SseEvent>(100);

        // SSE 연결 태스크
        let sse = self.sse_client.clone();
        let sid = session_id.to_string();
        tokio::spawn(async move {
            sse.connect(&sid, tx).await
        });

        // 이벤트 처리 루프
        let queue = self.queue.clone();
        let notifier = self.notifier.clone();
        while let Some(event) = rx.recv().await {
            match event {
                SseEvent::Suggestion(s) => {
                    queue.push(s.clone()).await;
                    notifier.notify(&s).await?;
                }
                SseEvent::Heartbeat { .. } => {
                    tracing::debug!("Heartbeat received");
                }
                SseEvent::Error(msg) => {
                    tracing::warn!("SSE error: {}", msg);
                }
                _ => {}
            }
        }

        Ok(())
    }
}
```

### PriorityQueue (queue.rs)

`BTreeSet` 기반 우선순위 큐:

```rust
pub struct PriorityQueue {
    queue: RwLock<BTreeSet<PrioritizedSuggestion>>,
    max_size: usize,
}

#[derive(Eq, PartialEq)]
struct PrioritizedSuggestion {
    priority_score: u32,  // 높을수록 우선
    suggestion: Suggestion,
}

impl Ord for PrioritizedSuggestion {
    fn cmp(&self, other: &Self) -> Ordering {
        // 우선순위 내림차순, 같으면 시간 오름차순
        other.priority_score.cmp(&self.priority_score)
            .then(self.suggestion.created_at.cmp(&other.suggestion.created_at))
    }
}

impl PriorityQueue {
    pub async fn push(&self, suggestion: Suggestion) {
        let mut queue = self.queue.write().await;

        let priority_score = self.calculate_score(&suggestion);
        queue.insert(PrioritizedSuggestion { priority_score, suggestion });

        // 최대 크기 초과 시 낮은 우선순위 제거
        while queue.len() > self.max_size {
            queue.pop_last();
        }
    }

    pub async fn pop(&self) -> Option<Suggestion> {
        let mut queue = self.queue.write().await;
        queue.pop_first().map(|p| p.suggestion)
    }

    fn calculate_score(&self, s: &Suggestion) -> u32 {
        let priority_weight = match s.priority {
            Priority::Critical => 1000,
            Priority::High => 750,
            Priority::Medium => 500,
            Priority::Low => 250,
        };

        let confidence_bonus = (s.confidence_score * 100.0) as u32;
        let relevance_bonus = (s.relevance_score * 100.0) as u32;
        let actionable_bonus = if s.is_actionable { 50 } else { 0 };

        priority_weight + confidence_bonus + relevance_bonus + actionable_bonus
    }
}
```

**우선순위 기준**:
| 항목 | 가중치 |
|------|--------|
| Critical 우선순위 | +1000 |
| High 우선순위 | +750 |
| Medium 우선순위 | +500 |
| Low 우선순위 | +250 |
| 신뢰도 (0-1) | +0~100 |
| 관련성 (0-1) | +0~100 |
| 실행 가능 | +50 |

### FeedbackSender (feedback.rs)

사용자 피드백 전송:

```rust
pub struct FeedbackSender {
    api_client: Arc<dyn ApiClient>,
    history: Arc<SuggestionHistory>,
}

impl FeedbackSender {
    pub async fn accept(&self, suggestion_id: &str) -> Result<(), CoreError> {
        self.api_client.send_feedback(suggestion_id, true).await?;
        self.history.record_feedback(suggestion_id, FeedbackType::Accepted).await;
        Ok(())
    }

    pub async fn reject(&self, suggestion_id: &str, reason: Option<&str>) -> Result<(), CoreError> {
        self.api_client.send_feedback(suggestion_id, false).await?;
        self.history.record_feedback(suggestion_id, FeedbackType::Rejected(reason.map(String::from))).await;
        Ok(())
    }

    pub async fn dismiss(&self, suggestion_id: &str) -> Result<(), CoreError> {
        self.history.record_feedback(suggestion_id, FeedbackType::Dismissed).await;
        Ok(())
    }
}
```

### SuggestionPresenter (presenter.rs)

UI 표시용 데이터 변환:

```rust
pub struct SuggestionPresenter;

pub struct SuggestionView {
    pub suggestion_id: String,
    pub title: String,
    pub body: String,
    pub priority_badge: String,
    pub action_buttons: Vec<ActionButton>,
    pub created_ago: String,
}

impl SuggestionPresenter {
    pub fn to_view(suggestion: &Suggestion) -> SuggestionView {
        SuggestionView {
            suggestion_id: suggestion.suggestion_id.clone(),
            title: Self::extract_title(&suggestion.content),
            body: Self::format_body(&suggestion.content),
            priority_badge: Self::priority_to_badge(&suggestion.priority),
            action_buttons: Self::create_actions(suggestion),
            created_ago: Self::format_time_ago(suggestion.created_at),
        }
    }

    fn priority_to_badge(priority: &Priority) -> String {
        match priority {
            Priority::Critical => "🔴 긴급".to_string(),
            Priority::High => "🟠 높음".to_string(),
            Priority::Medium => "🟡 보통".to_string(),
            Priority::Low => "🟢 낮음".to_string(),
        }
    }

    fn format_time_ago(created_at: DateTime<Utc>) -> String {
        let duration = Utc::now() - created_at;
        if duration.num_minutes() < 1 {
            "방금 전".to_string()
        } else if duration.num_hours() < 1 {
            format!("{}분 전", duration.num_minutes())
        } else if duration.num_days() < 1 {
            format!("{}시간 전", duration.num_hours())
        } else {
            format!("{}일 전", duration.num_days())
        }
    }
}
```

### SuggestionHistory (history.rs)

제안 이력 캐시:

```rust
pub struct SuggestionHistory {
    cache: RwLock<VecDeque<HistoryEntry>>,
    max_entries: usize,
}

pub struct HistoryEntry {
    pub suggestion: Suggestion,
    pub feedback: Option<FeedbackType>,
    pub received_at: DateTime<Utc>,
    pub actioned_at: Option<DateTime<Utc>>,
}

impl SuggestionHistory {
    pub async fn record(&self, suggestion: Suggestion) {
        let mut cache = self.cache.write().await;

        if cache.len() >= self.max_entries {
            cache.pop_front();
        }

        cache.push_back(HistoryEntry {
            suggestion,
            feedback: None,
            received_at: Utc::now(),
            actioned_at: None,
        });
    }

    pub async fn get_recent(&self, count: usize) -> Vec<HistoryEntry> {
        let cache = self.cache.read().await;
        cache.iter().rev().take(count).cloned().collect()
    }
}
```

## 제안 흐름

```
┌─────────────┐    ┌───────────────────┐    ┌───────────────┐
│   Server    │───▶│ SuggestionReceiver │───▶│ PriorityQueue │
│  (SSE)      │    │                   │    │               │
└─────────────┘    └───────────────────┘    └───────────────┘
                          │                        │
                          ▼                        ▼
                   ┌─────────────────┐    ┌───────────────────┐
                   │DesktopNotifier  │    │ SuggestionPresenter│
                   │    (알림)        │    │   (UI 표시)        │
                   └─────────────────┘    └───────────────────┘
                                                   │
                          ┌────────────────────────┘
                          ▼
                   ┌─────────────────┐    ┌───────────────────┐
                   │ FeedbackSender  │◀───│  사용자 반응       │
                   └─────────────────┘    └───────────────────┘
                          │
                          ▼
                   ┌─────────────────┐
                   │ SuggestionHistory│
                   └─────────────────┘
```

## 제안 타입

```rust
pub enum SuggestionType {
    WorkGuidance,      // 업무 안내
    RiskAlert,         // 위험 알림
    ProductivityTip,   // 생산성 팁
    ContextAwareness,  // 컨텍스트 인식
    ScheduleReminder,  // 일정 알림
}
```

## 의존성

- `oneshim-core`: 모델, 포트
- `oneshim-network`: SSE 클라이언트 (간접)
- `tokio`: 비동기 런타임, mpsc 채널

## 테스트

```rust
#[tokio::test]
async fn test_priority_queue_ordering() {
    let queue = PriorityQueue::new(50);

    // Low 먼저 추가
    let low = Suggestion {
        priority: Priority::Low,
        confidence_score: 0.5,
        // ...
    };
    queue.push(low).await;

    // High 나중에 추가
    let high = Suggestion {
        priority: Priority::High,
        confidence_score: 0.9,
        // ...
    };
    queue.push(high).await;

    // High가 먼저 나와야 함
    let first = queue.pop().await.unwrap();
    assert_eq!(first.priority, Priority::High);
}

#[test]
fn test_presenter_time_ago() {
    let now = Utc::now();

    // 30초 전
    let recent = now - Duration::seconds(30);
    assert_eq!(SuggestionPresenter::format_time_ago(recent), "방금 전");

    // 5분 전
    let minutes = now - Duration::minutes(5);
    assert_eq!(SuggestionPresenter::format_time_ago(minutes), "5분 전");
}
```
