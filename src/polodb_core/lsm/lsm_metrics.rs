use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone)]
pub struct LsmMetrics {
    inner: Arc<LsmMetricsInner>,
}

impl LsmMetrics {

    pub fn new() -> LsmMetrics {
        let inner = LsmMetricsInner::default();
        LsmMetrics {
            inner: Arc::new(inner),
        }
    }

    pub fn enable(&self) {
        self.inner.enable()
    }

    pub fn add_sync_count(&self) {
        self.inner.add_sync_count()
    }

    pub fn sync_count(&self) -> usize {
        self.inner.sync_count.load(Ordering::Relaxed)
    }

    pub fn add_minor_compact(&self) {
        self.inner.add_minor_compact();
    }

    pub fn minor_compact(&self) -> usize {
        self.inner.minor_compact()
    }

    pub fn set_free_segments_count(&self, count: usize) {
        self.inner.set_free_segments_count(count)
    }

    pub fn free_segments_count(&self) -> usize {
        self.inner.free_segments_count()
    }

}

macro_rules! test_enable {
    ($self:ident) => {
        if !$self.enable.load(Ordering::Relaxed) {
            return;
        }
    }
}

struct LsmMetricsInner {
    enable: AtomicBool,
    sync_count: AtomicUsize,
    minor_compact: AtomicUsize,
    free_segments_count: AtomicUsize,
}

impl LsmMetricsInner {

    #[inline]
    fn enable(&self) {
        self.enable.store(true, Ordering::Relaxed);
    }

    fn add_sync_count(&self) {
        test_enable!(self);
        self.sync_count.fetch_add(1, Ordering::Relaxed);
    }

    fn add_minor_compact(&self) {
        test_enable!(self);
        self.minor_compact.fetch_add(1, Ordering::Relaxed);
    }

    fn minor_compact(&self) -> usize {
        self.minor_compact.load(Ordering::Relaxed)
    }

    fn set_free_segments_count(&self, count: usize) {
        self.free_segments_count.store(count, Ordering::Relaxed);
    }

    fn free_segments_count(&self) -> usize {
        self.free_segments_count.load(Ordering::Relaxed)
    }

}

impl Default for LsmMetricsInner {

    fn default() -> Self {
        LsmMetricsInner {
            enable: AtomicBool::new(false),
            sync_count: AtomicUsize::new(0),
            minor_compact: AtomicUsize::new(0),
            free_segments_count: AtomicUsize::new(0),
        }
    }

}
