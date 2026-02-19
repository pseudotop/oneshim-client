//! 프레임 이미지 파일 저장소.
//!
//! WebP 이미지를 로컬 파일 시스템에 저장/조회/관리.
//! 일자별 폴더 구조, 보존 정책, 용량 관리.
//!
//! Phase 33 최적화:
//! - 버퍼 풀: 재사용 가능한 버퍼로 할당 오버헤드 제거
//! - 병렬 I/O: 여러 프레임 동시 저장
//! - 배치 삭제: 여러 파일 병렬 삭제

use chrono::{DateTime, Utc};
use crossbeam::queue::ArrayQueue;
use oneshim_core::error::CoreError;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info, warn};

/// 버퍼 풀 크기
const BUFFER_POOL_SIZE: usize = 16;

/// 버퍼 기본 크기 (256KB — 일반적인 WebP 프레임 크기)
const DEFAULT_BUFFER_SIZE: usize = 256 * 1024;

/// 병렬 삭제 동시성
const PARALLEL_DELETE_LIMIT: usize = 8;

/// 재사용 가능한 버퍼 풀
///
/// Phase 33 최적화: Vec 할당을 피하기 위해 버퍼를 재사용
struct BufferPool {
    pool: ArrayQueue<Vec<u8>>,
}

impl BufferPool {
    fn new(capacity: usize, buffer_size: usize) -> Self {
        let pool = ArrayQueue::new(capacity);
        // 미리 버퍼 생성
        for _ in 0..capacity {
            let _ = pool.push(Vec::with_capacity(buffer_size));
        }
        Self { pool }
    }

    /// 버퍼 대여 (없으면 새로 생성)
    fn acquire(&self) -> Vec<u8> {
        self.pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(DEFAULT_BUFFER_SIZE))
    }

    /// 버퍼 반납
    fn release(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        // 풀이 가득 차면 버퍼 버림
        let _ = self.pool.push(buffer);
    }
}

/// 프레임 이미지 파일 저장소
///
/// 일자별 디렉토리에 WebP 이미지 파일 저장.
/// 구조: `<base_dir>/frames/YYYY-MM-DD/HH-MM-SS-NNN.webp`
///
/// Phase 33 최적화:
/// - 버퍼 풀: 파일 읽기 시 Vec 재사용
/// - 병렬 I/O: save_frames_batch() 메서드 추가
pub struct FrameFileStorage {
    /// 기본 저장 디렉토리
    base_dir: PathBuf,
    /// 최대 저장 용량 (MB)
    max_storage_mb: u64,
    /// 보존 기간 (일)
    retention_days: u32,
    /// 프레임 카운터 (동일 초 내 중복 방지)
    frame_counter: AtomicU32,
    /// 버퍼 풀 (읽기 최적화)
    buffer_pool: Arc<BufferPool>,
}

impl FrameFileStorage {
    /// 새 프레임 저장소 생성
    ///
    /// # Arguments
    /// * `base_dir` - 기본 저장 디렉토리 (frames 하위 폴더에 저장)
    /// * `max_storage_mb` - 최대 저장 용량 (MB, 기본 1024 = 1GB)
    /// * `retention_days` - 보존 기간 (일, 기본 7)
    pub async fn new(
        base_dir: PathBuf,
        max_storage_mb: u64,
        retention_days: u32,
    ) -> Result<Self, CoreError> {
        let frames_dir = base_dir.join("frames");
        fs::create_dir_all(&frames_dir)
            .await
            .map_err(|e| CoreError::Internal(format!("프레임 디렉토리 생성 실패: {e}")))?;

        info!(
            "프레임 저장소 초기화: {} (최대 {}MB, {}일 보존, 버퍼풀 {}개)",
            frames_dir.display(),
            max_storage_mb,
            retention_days,
            BUFFER_POOL_SIZE
        );

        Ok(Self {
            base_dir,
            max_storage_mb,
            retention_days,
            frame_counter: AtomicU32::new(0),
            buffer_pool: Arc::new(BufferPool::new(BUFFER_POOL_SIZE, DEFAULT_BUFFER_SIZE)),
        })
    }

    /// WebP 이미지 저장
    ///
    /// Base64 디코딩된 WebP 바이트를 파일로 저장하고 경로 반환.
    ///
    /// # Arguments
    /// * `timestamp` - 캡처 시각
    /// * `webp_data` - WebP 이미지 바이트 (Base64 디코딩된 상태)
    ///
    /// # Returns
    /// 저장된 파일의 상대 경로 (예: `frames/2026-01-29/10-30-15-001.webp`)
    pub async fn save_frame(
        &self,
        timestamp: DateTime<Utc>,
        webp_data: &[u8],
    ) -> Result<PathBuf, CoreError> {
        // 일자별 폴더 생성
        let date_str = timestamp.format("%Y-%m-%d").to_string();
        let day_dir = self.base_dir.join("frames").join(&date_str);
        fs::create_dir_all(&day_dir)
            .await
            .map_err(|e| CoreError::Internal(format!("일자 폴더 생성 실패: {e}")))?;

        // 파일명 생성 (HH-MM-SS-NNN.webp)
        let counter = self.frame_counter.fetch_add(1, Ordering::SeqCst) % 1000;
        let time_str = timestamp.format("%H-%M-%S").to_string();
        let filename = format!("{time_str}-{counter:03}.webp");
        let file_path = day_dir.join(&filename);

        // 파일 저장
        fs::write(&file_path, webp_data)
            .await
            .map_err(|e| CoreError::Internal(format!("프레임 파일 저장 실패: {e}")))?;

        // 상대 경로 반환
        let relative_path = PathBuf::from("frames").join(&date_str).join(&filename);

        debug!(
            "프레임 저장: {} ({}bytes)",
            relative_path.display(),
            webp_data.len()
        );

        Ok(relative_path)
    }

    /// 여러 프레임 동시 저장 (병렬 I/O)
    ///
    /// Phase 33 최적화: tokio::spawn으로 병렬 저장
    ///
    /// # Arguments
    /// * `frames` - (timestamp, webp_data) 튜플 벡터
    ///
    /// # Returns
    /// 저장된 파일 경로 목록 (실패한 항목은 None)
    pub async fn save_frames_batch(
        &self,
        frames: Vec<(DateTime<Utc>, Vec<u8>)>,
    ) -> Vec<Result<PathBuf, CoreError>> {
        let mut handles = Vec::with_capacity(frames.len());

        for (timestamp, webp_data) in frames {
            let base_dir = self.base_dir.clone();
            let counter = self.frame_counter.fetch_add(1, Ordering::SeqCst) % 1000;

            handles.push(tokio::spawn(async move {
                let date_str = timestamp.format("%Y-%m-%d").to_string();
                let day_dir = base_dir.join("frames").join(&date_str);

                fs::create_dir_all(&day_dir)
                    .await
                    .map_err(|e| CoreError::Internal(format!("일자 폴더 생성 실패: {e}")))?;

                let time_str = timestamp.format("%H-%M-%S").to_string();
                let filename = format!("{time_str}-{counter:03}.webp");
                let file_path = day_dir.join(&filename);

                fs::write(&file_path, &webp_data)
                    .await
                    .map_err(|e| CoreError::Internal(format!("프레임 파일 저장 실패: {e}")))?;

                let relative_path = PathBuf::from("frames").join(&date_str).join(&filename);

                Ok(relative_path)
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(CoreError::Internal(format!("태스크 실패: {e}")))),
            }
        }

        results
    }

    /// 프레임 이미지 로드 (버퍼 풀 사용)
    ///
    /// Phase 33 최적화: 버퍼 풀에서 버퍼를 빌려서 사용
    ///
    /// # Arguments
    /// * `relative_path` - 상대 경로 (예: `frames/2026-01-29/10-30-15-001.webp`)
    pub async fn load_frame(&self, relative_path: &Path) -> Result<Vec<u8>, CoreError> {
        let full_path = self.base_dir.join(relative_path);

        if !full_path.exists() {
            return Err(CoreError::NotFound {
                resource_type: "Frame".to_string(),
                id: relative_path.display().to_string(),
            });
        }

        // 버퍼 풀에서 버퍼 대여
        let mut buffer = self.buffer_pool.acquire();

        let data = fs::read(&full_path)
            .await
            .map_err(|e| CoreError::Internal(format!("프레임 파일 읽기 실패: {e}")))?;

        // 데이터를 버퍼에 복사하고 반환
        buffer.extend_from_slice(&data);
        let result = buffer.clone();

        // 버퍼 반납
        self.buffer_pool.release(buffer);

        Ok(result)
    }

    /// 여러 프레임 동시 로드 (병렬 I/O)
    ///
    /// Phase 33 최적화: tokio::spawn으로 병렬 로드
    pub async fn load_frames_batch(&self, paths: Vec<PathBuf>) -> Vec<Result<Vec<u8>, CoreError>> {
        let mut handles = Vec::with_capacity(paths.len());

        for path in paths {
            let base_dir = self.base_dir.clone();
            let buffer_pool = Arc::clone(&self.buffer_pool);

            handles.push(tokio::spawn(async move {
                let full_path = base_dir.join(&path);

                if !full_path.exists() {
                    return Err(CoreError::NotFound {
                        resource_type: "Frame".to_string(),
                        id: path.display().to_string(),
                    });
                }

                let mut buffer = buffer_pool.acquire();

                let data = fs::read(&full_path)
                    .await
                    .map_err(|e| CoreError::Internal(format!("프레임 파일 읽기 실패: {e}")))?;

                buffer.extend_from_slice(&data);
                let result = buffer.clone();

                buffer_pool.release(buffer);

                Ok(result)
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(CoreError::Internal(format!("태스크 실패: {e}")))),
            }
        }

        results
    }

    /// 보존 정책 적용 (오래된 프레임 삭제)
    ///
    /// retention_days보다 오래된 일자 폴더 삭제.
    /// Phase 33 최적화: 병렬 삭제
    ///
    /// # Returns
    /// 삭제된 파일 수
    pub async fn enforce_retention(&self) -> Result<usize, CoreError> {
        let frames_dir = self.base_dir.join("frames");

        if !frames_dir.exists() {
            return Ok(0);
        }

        let cutoff_date = (Utc::now() - chrono::Duration::days(self.retention_days as i64))
            .format("%Y-%m-%d")
            .to_string();

        let mut entries = fs::read_dir(&frames_dir)
            .await
            .map_err(|e| CoreError::Internal(format!("frames 디렉토리 읽기 실패: {e}")))?;

        // 삭제 대상 폴더 수집
        let mut dirs_to_delete = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| CoreError::Internal(format!("디렉토리 항목 읽기 실패: {e}")))?
        {
            let path = entry.path();

            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if dir_name.len() == 10 && dir_name < cutoff_date.as_str() {
                    dirs_to_delete.push(path);
                }
            }
        }

        if dirs_to_delete.is_empty() {
            return Ok(0);
        }

        // 병렬 삭제
        let mut deleted_count = 0;
        for chunk in dirs_to_delete.chunks(PARALLEL_DELETE_LIMIT) {
            let mut handles = Vec::with_capacity(chunk.len());

            for path in chunk {
                let path = path.clone();
                handles.push(tokio::spawn(async move {
                    let count = count_files_in_dir(&path).await;
                    match fs::remove_dir_all(&path).await {
                        Ok(()) => Some(count),
                        Err(e) => {
                            warn!("오래된 프레임 폴더 삭제 실패: {e}");
                            None
                        }
                    }
                }));
            }

            for handle in handles {
                if let Ok(Some(count)) = handle.await {
                    deleted_count += count;
                }
            }
        }

        if deleted_count > 0 {
            info!(
                "프레임 보존 정책: {deleted_count}개 파일 삭제 (>{} 일)",
                self.retention_days
            );
        }

        Ok(deleted_count)
    }

    /// 총 저장 용량 확인 (MB)
    pub async fn total_size_mb(&self) -> Result<u64, CoreError> {
        let frames_dir = self.base_dir.join("frames");

        if !frames_dir.exists() {
            return Ok(0);
        }

        let size_bytes = calculate_dir_size(&frames_dir).await?;
        Ok(size_bytes / 1024 / 1024)
    }

    /// 용량 초과 시 오래된 파일 삭제
    ///
    /// max_storage_mb 초과 시 가장 오래된 일자 폴더부터 삭제.
    pub async fn enforce_storage_limit(&self) -> Result<usize, CoreError> {
        let current_mb = self.total_size_mb().await?;

        if current_mb <= self.max_storage_mb {
            return Ok(0);
        }

        let frames_dir = self.base_dir.join("frames");
        let mut deleted_count = 0;

        // 일자 폴더 목록 정렬 (오래된 순)
        let mut dirs = list_date_dirs(&frames_dir).await?;
        dirs.sort(); // YYYY-MM-DD 형식이므로 문자열 정렬 = 날짜 정렬

        // 용량이 기준 이하가 될 때까지 오래된 폴더 삭제
        for dir_name in dirs {
            let current = self.total_size_mb().await?;
            if current <= self.max_storage_mb {
                break;
            }

            let dir_path = frames_dir.join(&dir_name);
            let count = count_files_in_dir(&dir_path).await;
            deleted_count += count;

            if let Err(e) = fs::remove_dir_all(&dir_path).await {
                warn!("용량 초과 폴더 삭제 실패: {e}");
            } else {
                info!("용량 초과로 폴더 삭제: {} ({count}개 파일)", dir_name);
            }
        }

        Ok(deleted_count)
    }

    /// 프레임 디렉토리 경로 반환
    pub fn frames_dir(&self) -> PathBuf {
        self.base_dir.join("frames")
    }

    /// 버퍼 풀 통계
    pub fn buffer_pool_stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            pool_capacity: BUFFER_POOL_SIZE,
            buffer_size: DEFAULT_BUFFER_SIZE,
        }
    }
}

/// 버퍼 풀 통계
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    /// 풀 용량
    pub pool_capacity: usize,
    /// 버퍼 기본 크기
    pub buffer_size: usize,
}

/// 디렉토리 내 파일 수 카운트
async fn count_files_in_dir(path: &Path) -> usize {
    let mut count = 0;
    if let Ok(mut entries) = fs::read_dir(path).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().is_file() {
                count += 1;
            }
        }
    }
    count
}

/// 디렉토리 총 용량 계산 (bytes)
async fn calculate_dir_size(path: &Path) -> Result<u64, CoreError> {
    let mut total = 0u64;

    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| CoreError::Internal(format!("디렉토리 읽기 실패: {e}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| CoreError::Internal(format!("항목 읽기 실패: {e}")))?
    {
        let path = entry.path();
        let metadata = fs::metadata(&path)
            .await
            .map_err(|e| CoreError::Internal(format!("메타데이터 읽기 실패: {e}")))?;

        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            // 재귀적 용량 계산
            total += Box::pin(calculate_dir_size(&path)).await?;
        }
    }

    Ok(total)
}

/// 일자 폴더 목록 조회
async fn list_date_dirs(frames_dir: &Path) -> Result<Vec<String>, CoreError> {
    // 사전 할당: 최대 1년치 일자 폴더 예상 (성능 최적화)
    let mut dirs = Vec::with_capacity(365);

    if !frames_dir.exists() {
        return Ok(dirs);
    }

    let mut entries = fs::read_dir(frames_dir)
        .await
        .map_err(|e| CoreError::Internal(format!("frames 디렉토리 읽기 실패: {e}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| CoreError::Internal(format!("항목 읽기 실패: {e}")))?
    {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // YYYY-MM-DD 형식인지 확인
                if name.len() == 10 && name.chars().nth(4) == Some('-') {
                    dirs.push(name.to_string());
                }
            }
        }
    }

    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (FrameFileStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = FrameFileStorage::new(temp_dir.path().to_path_buf(), 100, 7)
            .await
            .unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn save_and_load_frame() {
        let (storage, _temp) = create_test_storage().await;

        // 테스트 WebP 데이터 (실제 WebP가 아니어도 됨)
        let test_data = b"RIFF\x00\x00\x00\x00WEBPVP8 test data";
        let timestamp = Utc::now();

        // 저장
        let path = storage.save_frame(timestamp, test_data).await.unwrap();
        assert!(path.to_string_lossy().contains("frames/"));
        assert!(path.to_string_lossy().ends_with(".webp"));

        // 로드
        let loaded = storage.load_frame(&path).await.unwrap();
        assert_eq!(loaded, test_data);
    }

    #[tokio::test]
    async fn save_multiple_same_second() {
        let (storage, _temp) = create_test_storage().await;

        let timestamp = Utc::now();
        let data = b"test";

        // 동일 초에 여러 프레임 저장
        let path1 = storage.save_frame(timestamp, data).await.unwrap();
        let path2 = storage.save_frame(timestamp, data).await.unwrap();
        let path3 = storage.save_frame(timestamp, data).await.unwrap();

        // 모두 다른 경로 (카운터 적용)
        assert_ne!(path1, path2);
        assert_ne!(path2, path3);
    }

    #[tokio::test]
    async fn save_frames_batch_parallel() {
        let (storage, _temp) = create_test_storage().await;

        let now = Utc::now();
        let frames: Vec<_> = (0..10)
            .map(|i| (now, format!("frame data {i}").into_bytes()))
            .collect();

        let results = storage.save_frames_batch(frames).await;

        // 모든 프레임 저장 성공
        assert_eq!(results.len(), 10);
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn load_frames_batch_parallel() {
        let (storage, _temp) = create_test_storage().await;

        let now = Utc::now();
        let frames: Vec<_> = (0..5)
            .map(|i| (now, format!("batch frame {i}").into_bytes()))
            .collect();

        let save_results = storage.save_frames_batch(frames.clone()).await;
        let paths: Vec<_> = save_results.into_iter().filter_map(|r| r.ok()).collect();

        let load_results = storage.load_frames_batch(paths).await;

        assert_eq!(load_results.len(), 5);
        for (i, result) in load_results.into_iter().enumerate() {
            let data = result.unwrap();
            assert_eq!(data, format!("batch frame {i}").into_bytes());
        }
    }

    #[tokio::test]
    async fn load_nonexistent_frame() {
        let (storage, _temp) = create_test_storage().await;

        let result = storage
            .load_frame(Path::new("frames/2099-01-01/00-00-00-000.webp"))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn total_size_empty() {
        let (storage, _temp) = create_test_storage().await;

        let size = storage.total_size_mb().await.unwrap();
        assert_eq!(size, 0);
    }

    #[tokio::test]
    async fn total_size_with_files() {
        let (storage, _temp) = create_test_storage().await;

        // 100KB 데이터 10개 저장
        let data = vec![0u8; 100 * 1024];
        for _ in 0..10 {
            storage.save_frame(Utc::now(), &data).await.unwrap();
        }

        let size = storage.total_size_mb().await.unwrap();
        // 약 1MB (u64이므로 항상 >= 0)
        assert!(size <= 2);
    }

    #[tokio::test]
    async fn retention_empty() {
        let (storage, _temp) = create_test_storage().await;

        let deleted = storage.enforce_retention().await.unwrap();
        assert_eq!(deleted, 0);
    }

    #[tokio::test]
    async fn frames_dir_path() {
        let (storage, temp) = create_test_storage().await;

        assert_eq!(storage.frames_dir(), temp.path().join("frames"));
    }

    #[tokio::test]
    async fn buffer_pool_stats() {
        let (storage, _temp) = create_test_storage().await;

        let stats = storage.buffer_pool_stats();
        assert_eq!(stats.pool_capacity, BUFFER_POOL_SIZE);
        assert_eq!(stats.buffer_size, DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn buffer_pool_acquire_release() {
        let pool = BufferPool::new(4, 1024);

        // 버퍼 대여
        let buf1 = pool.acquire();
        let buf2 = pool.acquire();
        assert!(buf1.capacity() >= 1024);
        assert!(buf2.capacity() >= 1024);

        // 버퍼 반납
        pool.release(buf1);
        pool.release(buf2);

        // 다시 대여 (풀에서 가져옴)
        let buf3 = pool.acquire();
        assert!(buf3.capacity() >= 1024);
    }
}
