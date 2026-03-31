use oneshim_core::error::CoreError;
use thiserror::Error;

/// oneshim-analysis crate 전용 에러 타입 (ADR-001 §1)
#[derive(Debug, Error)]
pub enum AnalysisError {
    /// oneshim-core 에러를 투명하게 전달
    #[error(transparent)]
    Core(#[from] CoreError),

    /// 벡터 인덱스 (HNSW 등) 관련 에러
    #[error("vector index error: {0}")]
    VectorIndex(String),

    /// 클러스터링 알고리즘 실패 (GMM, HDBSCAN 등)
    #[error("clustering failed: {0}")]
    Clustering(String),

    /// LLM 서비스 에러
    #[error("LLM service error: {0}")]
    LlmService(String),

    /// 기타 내부 에러
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<AnalysisError> for CoreError {
    fn from(err: AnalysisError) -> Self {
        match err {
            AnalysisError::Core(e) => e,
            AnalysisError::VectorIndex(msg) => CoreError::Internal(msg),
            AnalysisError::Clustering(msg) => CoreError::Analysis(msg),
            AnalysisError::LlmService(msg) => CoreError::Analysis(msg),
            AnalysisError::Internal(msg) => CoreError::Internal(msg),
        }
    }
}
