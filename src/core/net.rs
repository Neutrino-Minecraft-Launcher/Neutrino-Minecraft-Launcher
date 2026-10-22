//! HTTP 下载：重试、SHA-1 校验、多线程并发、进度上报。

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};

use super::error::{CoreError, Result, io_err};
use super::hash::{hex_encode, sha1_matches};
use super::progress::Progress;

const USER_AGENT: &str = concat!("NeutrinoMCL/", env!("CARGO_PKG_VERSION"));

/// 单个文件的下载结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadOutcome {
    /// 本地文件已存在且校验通过，跳过。
    Cached,
    /// 实际下载了。
    Downloaded,
}

/// 一个待下载文件。
#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    /// 预期大小，仅在无 sha1 时用于判断缓存是否可用，也用于进度总量。
    pub size: u64,
}

impl DownloadJob {
    /// 构造一个下载任务。
    pub fn new(
        url: impl Into<String>,
        dest: impl Into<PathBuf>,
        sha1: Option<String>,
        size: u64,
    ) -> Self {
        Self {
            url: url.into(),
            dest: dest.into(),
            sha1,
            size,
        }
    }

    /// 这批任务的总字节数。
    pub fn total_size(jobs: &[Self]) -> u64 {
        jobs.iter().map(|job| job.size).sum()
    }
}

/// 一批下载的统计。
#[derive(Debug, Default, Clone, Copy)]
pub struct DownloadStats {
    pub files: usize,
    pub cached: usize,
    pub bytes: u64,
}

/// HTTP 下载器。内部持有一个连接池可复用的 agent，可安全地跨线程共享。
#[derive(Clone)]
pub struct Downloader {
    agent: ureq2::Agent,
    progress: Progress,
    retries: u32,
}

impl Downloader {
    /// 创建下载器。
    pub fn new(progress: Progress) -> Self {
        let agent = ureq2::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .build();

        Self {
            agent,
            progress,
            retries: 3,
        }
    }

    /// 设置失败重试次数（不含首次尝试）。
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// 取回原始字节。
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.retry(|| {
            let response = self.request(url)?;
            let mut buffer = Vec::new();
            response
                .into_reader()
                .read_to_end(&mut buffer)
                .map_err(|err| CoreError::Network(format!("读取 {url} 的响应失败：{err}")))?;
            Ok(buffer)
        })
    }

    /// 取回并解析 JSON。
    pub fn get_json<T: DeserializeOwned>(&self, url: &str, context: &str) -> Result<T> {
        let bytes = self.get_bytes(url)?;
        serde_json::from_slice(&bytes).map_err(|source| CoreError::Json {
            context: context.to_string(),
            source,
        })
    }

    /// 下载单个文件；本地已有且校验通过时直接跳过。
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        sha1: Option<&str>,
        size: u64,
    ) -> Result<DownloadOutcome> {
        let cached = match sha1 {
            Some(expected) => sha1_matches(dest, expected),
            None => {
                size > 0
                    && fs::metadata(dest)
                        .map(|meta| meta.len() == size)
                        .unwrap_or(false)
            }
        };

        if cached {
            self.progress.advance(size);
            return Ok(DownloadOutcome::Cached);
        }

        if let Some(parent) = dest.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|err| io_err(parent, err))?;
        }

        self.retry(|| self.fetch_to_file(url, dest, sha1))?;
        Ok(DownloadOutcome::Downloaded)
    }

    /// 并发下载一批文件，任意一个失败即中止其余任务。
    pub fn download_all(&self, jobs: &[DownloadJob], threads: usize) -> Result<DownloadStats> {
        if jobs.is_empty() {
            return Ok(DownloadStats::default());
        }

        let workers = threads.clamp(1, 32).min(jobs.len());
        let cursor = AtomicUsize::new(0);
        let failure: Mutex<Option<CoreError>> = Mutex::new(None);
        let stats: Mutex<DownloadStats> = Mutex::new(DownloadStats::default());

        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    loop {
                        let failed = failure.lock().map(|guard| guard.is_some()).unwrap_or(true);
                        if failed {
                            break;
                        }

                        let position = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(job) = jobs.get(position) else {
                            break;
                        };

                        match self.download(&job.url, &job.dest, job.sha1.as_deref(), job.size) {
                            Ok(outcome) => {
                                if let Ok(mut guard) = stats.lock() {
                                    guard.files += 1;
                                    guard.bytes += job.size;
                                    if outcome == DownloadOutcome::Cached {
                                        guard.cached += 1;
                                    }
                                }
                            }
                            Err(err) => {
                                if let Ok(mut guard) = failure.lock()
                                    && guard.is_none()
                                {
                                    *guard = Some(err);
                                }
                                break;
                            }
                        }
                    }
                });
            }
        });

        if let Some(err) = failure.into_inner().ok().flatten() {
            return Err(err);
        }

        Ok(stats.into_inner().unwrap_or_default())
    }

    fn request(&self, url: &str) -> Result<ureq2::Response> {
        match self.agent.get(url).call() {
            Ok(response) => Ok(response),
            Err(ureq2::Error::Status(status, _)) => Err(CoreError::HttpStatus {
                status,
                url: url.to_string(),
            }),
            Err(ureq2::Error::Transport(transport)) => {
                Err(CoreError::Network(format!("{transport}（{url}）")))
            }
        }
    }

    fn fetch_to_file(&self, url: &str, dest: &Path, sha1: Option<&str>) -> Result<()> {
        let response = self.request(url)?;

        let mut tmp_name = dest.as_os_str().to_owned();
        tmp_name.push(".part");
        let tmp = PathBuf::from(tmp_name);

        let mut hasher = Sha1::new();
        {
            let mut reader = self.progress.counting_reader(response.into_reader());
            let mut file = File::create(&tmp).map_err(|err| io_err(&tmp, err))?;
            let mut buffer = [0u8; 64 * 1024];

            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|err| CoreError::Network(format!("下载 {url} 时中断：{err}")))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read])
                    .map_err(|err| io_err(&tmp, err))?;
            }

            file.flush().map_err(|err| io_err(&tmp, err))?;
        }

        let actual = hex_encode(&hasher.finalize());
        if let Some(expected) = sha1
            && !actual.eq_ignore_ascii_case(expected.trim())
        {
            let _ = fs::remove_file(&tmp);
            return Err(CoreError::HashMismatch {
                path: dest.to_path_buf(),
                expected: expected.to_string(),
                actual,
            });
        }

        // Windows 的 rename 可以覆盖已存在文件，这里先删是为了兼容个别只读场景。
        if dest.exists() {
            let _ = fs::remove_file(dest);
        }
        fs::rename(&tmp, dest).map_err(|err| io_err(dest, err))?;
        Ok(())
    }

    fn retry<T>(&self, mut operation: impl FnMut() -> Result<T>) -> Result<T> {
        let mut delay = Duration::from_millis(400);
        let mut attempt = 0;

        loop {
            match operation() {
                Ok(value) => return Ok(value),
                Err(err) => {
                    if attempt >= self.retries || !is_retryable(&err) {
                        return Err(err);
                    }
                    attempt += 1;
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(8));
                }
            }
        }
    }
}

fn is_retryable(err: &CoreError) -> bool {
    match err {
        CoreError::HttpStatus { status, .. } => *status >= 500 || *status == 429,
        CoreError::Network(_) => true,
        // 校验失败通常是传输被截断或镜像返回了错误页面，重下一次可能就好了
        CoreError::HashMismatch { .. } => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_size_sums_jobs() {
        let jobs = vec![
            DownloadJob::new("https://a", "a.jar", None, 100),
            DownloadJob::new("https://b", "b.jar", None, 250),
        ];
        assert_eq!(DownloadJob::total_size(&jobs), 350);
        assert_eq!(DownloadJob::total_size(&[]), 0);
    }

    #[test]
    fn client_errors_are_not_retried() {
        assert!(!is_retryable(&CoreError::HttpStatus {
            status: 404,
            url: "https://a".to_string(),
        }));
        assert!(is_retryable(&CoreError::HttpStatus {
            status: 503,
            url: "https://a".to_string(),
        }));
        assert!(is_retryable(&CoreError::Network("timed out".to_string())));
    }

    #[test]
    fn cached_file_is_reused_without_network() {
        let dir = std::env::temp_dir().join("neutrino-downloader-test");
        let _ = fs::create_dir_all(&dir);
        let dest = dir.join("payload.bin");
        fs::write(&dest, b"abc").unwrap();

        let expected = super::super::hash::sha1_bytes(b"abc");
        let downloader = Downloader::new(Progress::silent());
        // URL 故意写成不可达地址：只要缓存命中就不应该发起请求
        let outcome = downloader
            .download("https://example.invalid/nope", &dest, Some(&expected), 3)
            .unwrap();
        assert_eq!(outcome, DownloadOutcome::Cached);

        let _ = fs::remove_dir_all(&dir);
    }
}
