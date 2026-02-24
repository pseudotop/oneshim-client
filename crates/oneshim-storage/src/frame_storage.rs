//!
//!

use chrono::{DateTime, Utc};
use crossbeam::queue::ArrayQueue;
use oneshim_core::error::CoreError;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info, warn};

const BUFFER_POOL_SIZE: usize = 16;

const DEFAULT_BUFFER_SIZE: usize = 256 * 1024;

const PARALLEL_DELETE_LIMIT: usize = 8;

///
struct BufferPool {
    pool: ArrayQueue<Vec<u8>>,
}

impl BufferPool {
    fn new(capacity: usize, buffer_size: usize) -> Self {
        let pool = ArrayQueue::new(capacity);
        for _ in 0..capacity {
            let _ = pool.push(Vec::with_capacity(buffer_size));
        }
        Self { pool }
    }

    fn acquire(&self) -> Vec<u8> {
        self.pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(DEFAULT_BUFFER_SIZE))
    }

    fn release(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        let _ = self.pool.push(buffer);
    }
}

///
///
pub struct FrameFileStorage {
    base_dir: PathBuf,
    max_storage_mb: u64,
    retention_days: u32,
    frame_counter: AtomicU32,
    buffer_pool: Arc<BufferPool>,
}

impl FrameFileStorage {
    ///
    /// # Arguments
    pub async fn new(
        base_dir: PathBuf,
        max_storage_mb: u64,
        retention_days: u32,
    ) -> Result<Self, CoreError> {
        let frames_dir = base_dir.join("frames");
        fs::create_dir_all(&frames_dir)
            .await
            .map_err(|e| CoreError::Internal(format!("frame 디렉토리 create failure: {e}")))?;

        info!(
            "frame save소 initialize: {} (최대 {}MB, {}일 보존, 버퍼풀 {}개)",
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

    ///
    ///
    /// # Arguments
    ///
    /// # Returns
    pub async fn save_frame(
        &self,
        timestamp: DateTime<Utc>,
        webp_data: &[u8],
    ) -> Result<PathBuf, CoreError> {
        let date_str = timestamp.format("%Y-%m-%d").to_string();
        let day_dir = self.base_dir.join("frames").join(&date_str);
        fs::create_dir_all(&day_dir)
            .await
            .map_err(|e| CoreError::Internal(format!("일자 folder create failure: {e}")))?;

        let counter = self.frame_counter.fetch_add(1, Ordering::SeqCst) % 1000;
        let time_str = timestamp.format("%H-%M-%S").to_string();
        let filename = format!("{time_str}-{counter:03}.webp");
        let file_path = day_dir.join(&filename);

        fs::write(&file_path, webp_data)
            .await
            .map_err(|e| CoreError::Internal(format!("frame file save failure: {e}")))?;

        let relative_path = PathBuf::from("frames").join(&date_str).join(&filename);

        debug!(
            "frame save: {} ({}bytes)",
            relative_path.display(),
            webp_data.len()
        );

        Ok(relative_path)
    }

    ///
    ///
    /// # Arguments
    ///
    /// # Returns
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
                    .map_err(|e| CoreError::Internal(format!("일자 folder create failure: {e}")))?;

                let time_str = timestamp.format("%H-%M-%S").to_string();
                let filename = format!("{time_str}-{counter:03}.webp");
                let file_path = day_dir.join(&filename);

                fs::write(&file_path, &webp_data)
                    .await
                    .map_err(|e| CoreError::Internal(format!("frame file save failure: {e}")))?;

                let relative_path = PathBuf::from("frames").join(&date_str).join(&filename);

                Ok(relative_path)
            }));
        }

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(Err(CoreError::Internal(format!("태스크 failure: {e}")))),
            }
        }

        results
    }

    ///
    ///
    /// # Arguments
    pub async fn load_frame(&self, relative_path: &Path) -> Result<Vec<u8>, CoreError> {
        let full_path = self.base_dir.join(relative_path);

        if !full_path.exists() {
            return Err(CoreError::NotFound {
                resource_type: "Frame".to_string(),
                id: relative_path.display().to_string(),
            });
        }

        let mut buffer = self.buffer_pool.acquire();

        let data = fs::read(&full_path)
            .await
            .map_err(|e| CoreError::Internal(format!("frame file read failure: {e}")))?;

        buffer.extend_from_slice(&data);
        let result = buffer.clone();

        self.buffer_pool.release(buffer);

        Ok(result)
    }

    ///
    pub async fn load_latest_frame(&self) -> Result<Option<(Vec<u8>, String)>, CoreError> {
        let frames_dir = self.base_dir.join("frames");
        if !frames_dir.exists() {
            return Ok(None);
        }

        let mut day_dirs = list_date_dirs(&frames_dir).await?;
        day_dirs.sort_by(|a, b| b.cmp(a));

        for day in day_dirs {
            let day_path = frames_dir.join(&day);
            if !day_path.exists() {
                continue;
            }

            let mut files = Vec::new();
            let mut entries = fs::read_dir(&day_path)
                .await
                .map_err(|e| CoreError::Internal(format!("frame folder read failure: {e}")))?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| CoreError::Internal(format!("frame 항목 read failure: {e}")))?
            {
                let path = entry.path();
                if path.is_file() {
                    files.push(path);
                }
            }

            if files.is_empty() {
                continue;
            }

            files.sort_by(|a, b| {
                let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                b_name.cmp(a_name)
            });

            let latest = &files[0];
            let Some(filename) = latest.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let relative_path = PathBuf::from("frames").join(&day).join(filename);
            let bytes = self.load_frame(&relative_path).await?;
            let format = latest
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| "webp".to_string());
            return Ok(Some((bytes, format)));
        }

        Ok(None)
    }

    ///
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
                    .map_err(|e| CoreError::Internal(format!("frame file read failure: {e}")))?;

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
                Err(e) => results.push(Err(CoreError::Internal(format!("태스크 failure: {e}")))),
            }
        }

        results
    }

    ///
    ///
    /// # Returns
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
            .map_err(|e| CoreError::Internal(format!("frames 디렉토리 read failure: {e}")))?;

        let mut dirs_to_delete = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| CoreError::Internal(format!("디렉토리 항목 read failure: {e}")))?
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
                            warn!("frame folder delete failure: {e}");
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
                "frame 보존 policy: {deleted_count}개 file delete (>{} 일)",
                self.retention_days
            );
        }

        Ok(deleted_count)
    }

    pub async fn total_size_mb(&self) -> Result<u64, CoreError> {
        let frames_dir = self.base_dir.join("frames");

        if !frames_dir.exists() {
            return Ok(0);
        }

        let size_bytes = calculate_dir_size(&frames_dir).await?;
        Ok(size_bytes / 1024 / 1024)
    }

    ///
    pub async fn enforce_storage_limit(&self) -> Result<usize, CoreError> {
        let current_mb = self.total_size_mb().await?;

        if current_mb <= self.max_storage_mb {
            return Ok(0);
        }

        let frames_dir = self.base_dir.join("frames");
        let mut deleted_count = 0;

        let mut dirs = list_date_dirs(&frames_dir).await?;
        dirs.sort(); // YYYY-MM-DD =
        for dir_name in dirs {
            let current = self.total_size_mb().await?;
            if current <= self.max_storage_mb {
                break;
            }

            let dir_path = frames_dir.join(&dir_name);
            let count = count_files_in_dir(&dir_path).await;
            deleted_count += count;

            if let Err(e) = fs::remove_dir_all(&dir_path).await {
                warn!("s folder delete failure: {e}");
            } else {
                info!("s folder delete: {} ({count}items file)", dir_name);
            }
        }

        Ok(deleted_count)
    }

    pub fn frames_dir(&self) -> PathBuf {
        self.base_dir.join("frames")
    }

    pub fn buffer_pool_stats(&self) -> BufferPoolStats {
        BufferPoolStats {
            pool_capacity: BUFFER_POOL_SIZE,
            buffer_size: DEFAULT_BUFFER_SIZE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    pub pool_capacity: usize,
    pub buffer_size: usize,
}

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

async fn calculate_dir_size(path: &Path) -> Result<u64, CoreError> {
    let mut total = 0u64;

    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| CoreError::Internal(format!("디렉토리 read failure: {e}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| CoreError::Internal(format!("항목 read failure: {e}")))?
    {
        let path = entry.path();
        let metadata = fs::metadata(&path)
            .await
            .map_err(|e| CoreError::Internal(format!("메타데이터 read failure: {e}")))?;

        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            total += Box::pin(calculate_dir_size(&path)).await?;
        }
    }

    Ok(total)
}

async fn list_date_dirs(frames_dir: &Path) -> Result<Vec<String>, CoreError> {
    let mut dirs = Vec::with_capacity(365);

    if !frames_dir.exists() {
        return Ok(dirs);
    }

    let mut entries = fs::read_dir(frames_dir)
        .await
        .map_err(|e| CoreError::Internal(format!("frames 디렉토리 read failure: {e}")))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| CoreError::Internal(format!("항목 read failure: {e}")))?
    {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
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

        let test_data = b"RIFF\x00\x00\x00\x00WEBPVP8 test data";
        let timestamp = Utc::now();

        let path = storage.save_frame(timestamp, test_data).await.unwrap();
        assert!(path.to_string_lossy().contains("frames/"));
        assert!(path.to_string_lossy().ends_with(".webp"));

        let loaded = storage.load_frame(&path).await.unwrap();
        assert_eq!(loaded, test_data);
    }

    #[tokio::test]
    async fn load_latest_frame_returns_most_recent_file() {
        let (storage, _temp) = create_test_storage().await;

        let t1 = Utc::now() - chrono::Duration::seconds(1);
        let t2 = Utc::now();

        storage.save_frame(t1, b"older-frame").await.unwrap();
        storage.save_frame(t2, b"newer-frame").await.unwrap();

        let latest = storage.load_latest_frame().await.unwrap().unwrap();
        assert_eq!(latest.0, b"newer-frame");
        assert_eq!(latest.1, "webp");
    }

    #[tokio::test]
    async fn save_multiple_same_second() {
        let (storage, _temp) = create_test_storage().await;

        let timestamp = Utc::now();
        let data = b"test";

        let path1 = storage.save_frame(timestamp, data).await.unwrap();
        let path2 = storage.save_frame(timestamp, data).await.unwrap();
        let path3 = storage.save_frame(timestamp, data).await.unwrap();

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

        let data = vec![0u8; 100 * 1024];
        for _ in 0..10 {
            storage.save_frame(Utc::now(), &data).await.unwrap();
        }

        let size = storage.total_size_mb().await.unwrap();
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

        let buf1 = pool.acquire();
        let buf2 = pool.acquire();
        assert!(buf1.capacity() >= 1024);
        assert!(buf2.capacity() >= 1024);

        pool.release(buf1);
        pool.release(buf2);

        let buf3 = pool.acquire();
        assert!(buf3.capacity() >= 1024);
    }
}
