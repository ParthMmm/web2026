use rune::{Image, ImageCache, Pixels};
use std::{path::Path, sync::Arc};

/// Decoded images kept for one purpose (grid thumbnails or the large preview):
/// an LRU of at most `capacity` entries, with at most `concurrency` decodes in
/// flight. A pending decode is never evicted, so the bound on in-flight work
/// holds even for tiles that scrolled away before their decode finished.
pub(super) struct ImageSlots {
    /// Least recently requested first.
    entries: Vec<Slot>,
    capacity: usize,
    concurrency: usize,
    pub(super) failures: usize,
    pub(super) peak_entries: usize,
    pub(super) peak_loading: usize,
}

struct Slot {
    path: Arc<Path>,
    image: Image,
    settled: bool,
}

impl ImageSlots {
    pub(super) fn new(capacity: usize, concurrency: usize) -> Self {
        Self {
            entries: Vec::new(),
            capacity,
            concurrency,
            failures: 0,
            peak_entries: 0,
            peak_loading: 0,
        }
    }

    pub(super) fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    fn loading(&self) -> usize {
        self.entries.iter().filter(|slot| !slot.settled).count()
    }

    /// Marks `path` as most recently used, starting its decode when a decode
    /// slot is free. Returns false when the request has to wait for one.
    pub(super) fn request(&mut self, cache: &ImageCache, path: &Arc<Path>) -> bool {
        if let Some(index) = self.entries.iter().position(|slot| slot.path == *path) {
            let slot = self.entries.remove(index);
            self.entries.push(slot);
            return true;
        }
        if self.loading() >= self.concurrency {
            return false;
        }
        if self.entries.len() >= self.capacity {
            let Some(index) = self.entries.iter().position(|slot| slot.settled) else {
                return false;
            };
            self.entries.remove(index);
        }
        self.entries.push(Slot {
            path: path.clone(),
            image: cache.file(path, 0.0, 1.0),
            settled: false,
        });
        self.peak_entries = self.peak_entries.max(self.entries.len());
        self.peak_loading = self.peak_loading.max(self.loading());
        true
    }

    /// Keeps an image made in memory, as the demo's are, under `path`. It
    /// replaces the oldest settled entry when the slots are full.
    pub(super) fn insert(&mut self, path: Arc<Path>, image: Image) {
        if self.entries.iter().any(|slot| slot.path == path) {
            return;
        }
        if self.entries.len() >= self.capacity
            && let Some(index) = self.entries.iter().position(|slot| slot.settled)
        {
            self.entries.remove(index);
        }
        let settled = image.is_ready();
        self.entries.push(Slot {
            path,
            image,
            settled,
        });
    }

    /// Records decodes that finished since the last call, and returns the
    /// paths that settled with whether each decoded.
    pub(super) fn settle(&mut self) -> Vec<(Arc<Path>, bool)> {
        let mut settled = Vec::new();
        for slot in self.entries.iter_mut().filter(|slot| !slot.settled) {
            if slot.image.is_ready() {
                slot.settled = true;
                let decoded = slot.image.error().is_none();
                self.failures += usize::from(!decoded);
                settled.push((slot.path.clone(), decoded));
            }
        }
        settled
    }

    /// The decoded pixels for `path`, if its decode finished and succeeded.
    pub(super) fn pixels(&self, path: &Path) -> Option<&Pixels> {
        self.entries
            .iter()
            .find(|slot| slot.settled && *slot.path == *path && slot.image.error().is_none())
            .map(|slot| slot.image.pixels())
    }
}
