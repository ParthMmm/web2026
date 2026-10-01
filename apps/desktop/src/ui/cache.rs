use gpui_kit::{
    App, AppContext as _, Entity, ImageCache, ImageCacheError, RenderImage, Resource,
    RetainAllImageCache, Window,
};
use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

/// An LRU image cache with a bound on concurrent decodes.
pub(super) struct BoundedCache {
    inner: Entity<RetainAllImageCache>,
    recent: VecDeque<Resource>,
    capacity: usize,
    loading: HashSet<Resource>,
    concurrency: usize,
    pub(super) failures: usize,
    pub(super) peak_entries: usize,
    pub(super) peak_loading: usize,
}

impl BoundedCache {
    pub(super) fn new(capacity: usize, concurrency: usize, cx: &mut App) -> Entity<Self> {
        let inner = RetainAllImageCache::new(cx);
        cx.new(|_| Self {
            inner,
            recent: VecDeque::new(),
            capacity,
            loading: HashSet::new(),
            concurrency,
            failures: 0,
            peak_entries: 0,
            peak_loading: 0,
        })
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn entries(&self, cx: &App) -> usize {
        self.inner.read(cx).len()
    }
}

impl ImageCache for BoundedCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        // Settle off-screen loads too. Never evict a pending load: GPUI's shared
        // task survives removal and would otherwise escape our concurrency bound.
        for pending in self.loading.clone() {
            if let Some(result) = self
                .inner
                .update(cx, |cache, cx| cache.load(&pending, window, cx))
            {
                self.loading.remove(&pending);
                self.failures += usize::from(result.is_err());
            }
        }
        if let Some(index) = self.recent.iter().position(|r| r == resource) {
            self.recent.remove(index);
        } else {
            if self.loading.len() >= self.concurrency {
                return None;
            }
            if self.recent.len() == self.capacity {
                let index = self.recent.iter().position(|r| !self.loading.contains(r))?;
                let old = self.recent.remove(index)?;
                self.inner
                    .update(cx, |cache, cx| cache.remove(&old, window, cx));
            }
        }
        self.recent.push_back(resource.clone());
        let result = self
            .inner
            .update(cx, |cache, cx| cache.load(resource, window, cx));
        if result.is_none() {
            self.loading.insert(resource.clone());
        }
        self.peak_entries = self.peak_entries.max(self.inner.read(cx).len());
        self.peak_loading = self.peak_loading.max(self.loading.len());
        result
    }
}
