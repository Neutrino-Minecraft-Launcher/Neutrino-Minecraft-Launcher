//! 下载进度上报。
//!
//! 设计目标：安装逻辑只负责 `advance(bytes)`，不关心谁在监听；
//! CLI 用它画进度条，GUI 以后可以用它更新 Slint 属性，两者都不需要改核心代码。
//!
//! 进度对象是 `Clone + Send + Sync` 的，可以安全地共享给多个下载线程。

use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 内部节流间隔：最多每 60ms 触发一次回调，避免每读 8KB 就唤醒一次监听者。
const EMIT_INTERVAL_MS: u64 = 60;

/// 一次进度快照。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    /// 当前阶段名，例如“下载依赖库”。
    pub stage: String,
    /// 当前阶段已完成量（字节）。
    pub done: u64,
    /// 当前阶段总量（字节），0 表示未知。
    pub total: u64,
}

impl ProgressSnapshot {
    /// 完成比例，范围 0.0 ~ 1.0。
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).clamp(0.0, 1.0)
        }
    }
}

/// 进度回调。
pub type ProgressSink = Arc<dyn Fn(ProgressSnapshot) + Send + Sync>;

/// 可克隆的进度上报器。
#[derive(Clone, Default)]
pub struct Progress {
    inner: Arc<ProgressInner>,
}

#[derive(Default)]
struct ProgressInner {
    stage: Mutex<String>,
    done: AtomicU64,
    total: AtomicU64,
    sink: Option<ProgressSink>,
    start: Option<Instant>,
    last_emit_ms: AtomicU64,
}

impl Progress {
    /// 创建一个什么都不做的进度对象。
    pub fn silent() -> Self {
        Self::default()
    }

    /// 创建带回调的进度对象。
    pub fn new(sink: impl Fn(ProgressSnapshot) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(ProgressInner {
                stage: Mutex::new(String::new()),
                done: AtomicU64::new(0),
                total: AtomicU64::new(0),
                sink: Some(Arc::new(sink)),
                start: Some(Instant::now()),
                last_emit_ms: AtomicU64::new(0),
            }),
        }
    }

    /// 开始一个新阶段，`total` 为该阶段总字节数（未知时传 0）。
    pub fn begin_stage(&self, stage: impl Into<String>, total: u64) {
        if let Ok(mut guard) = self.inner.stage.lock() {
            *guard = stage.into();
        }
        self.inner.done.store(0, Ordering::Relaxed);
        self.inner.total.store(total, Ordering::Relaxed);
        self.emit(true);
    }

    /// 在阶段总量上追加（例如先统计依赖库、后统计资源文件）。
    pub fn add_total(&self, extra: u64) {
        self.inner.total.fetch_add(extra, Ordering::Relaxed);
    }

    /// 结束当前阶段：把进度顶满并强制上报一次。
    pub fn finish_stage(&self) {
        let total = self.inner.total.load(Ordering::Relaxed);
        self.inner.done.store(total, Ordering::Relaxed);
        self.emit(true);
    }

    /// 上报一段已完成字节。
    pub fn advance(&self, bytes: u64) {
        if bytes == 0 {
            return;
        }
        self.inner.done.fetch_add(bytes, Ordering::Relaxed);
        self.emit(false);
    }

    /// 读取当前快照。
    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            stage: self
                .inner
                .stage
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default(),
            done: self.inner.done.load(Ordering::Relaxed),
            total: self.inner.total.load(Ordering::Relaxed),
        }
    }

    /// 用一个会自动上报进度的读取器包住任意 `Read`。
    pub fn counting_reader<R: Read>(&self, inner: R) -> CountingReader<R> {
        CountingReader {
            inner,
            progress: self.clone(),
        }
    }

    fn emit(&self, force: bool) {
        let Some(sink) = self.inner.sink.as_ref() else {
            return;
        };

        let now_ms = self
            .inner
            .start
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);

        if force {
            self.inner.last_emit_ms.store(now_ms, Ordering::Relaxed);
        } else {
            let last = self.inner.last_emit_ms.load(Ordering::Relaxed);
            if now_ms.saturating_sub(last) < EMIT_INTERVAL_MS {
                return;
            }
            if self
                .inner
                .last_emit_ms
                .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                .is_err()
            {
                return;
            }
        }

        sink(self.snapshot());
    }
}

/// 读取时自动累加进度的包装器。
pub struct CountingReader<R> {
    inner: R,
    progress: Progress,
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.progress.advance(read as u64);
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_is_clamped() {
        let snap = ProgressSnapshot {
            stage: "x".into(),
            done: 5,
            total: 10,
        };
        assert!((snap.ratio() - 0.5).abs() < f64::EPSILON);

        let empty = ProgressSnapshot::default();
        assert_eq!(empty.ratio(), 0.0);

        let overflow = ProgressSnapshot {
            stage: "x".into(),
            done: 30,
            total: 10,
        };
        assert_eq!(overflow.ratio(), 1.0);
    }

    #[test]
    fn stage_transitions_are_reported() {
        let seen: Arc<Mutex<Vec<ProgressSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let progress = Progress::new(move |snap| sink.lock().unwrap().push(snap));

        progress.begin_stage("下载依赖库", 100);
        progress.advance(40);
        progress.finish_stage();

        let events = seen.lock().unwrap();
        assert_eq!(events.first().unwrap().stage, "下载依赖库");
        assert_eq!(events.first().unwrap().total, 100);
        // advance 可能被节流，但 finish_stage 一定会补上最终值
        let last = events.last().unwrap();
        assert_eq!(last.done, 100);
        assert_eq!(last.total, 100);
    }

    #[test]
    fn silent_progress_does_not_panic() {
        let progress = Progress::silent();
        progress.begin_stage("x", 10);
        progress.advance(10);
        progress.finish_stage();
        assert_eq!(progress.snapshot().done, 10);
    }
}
