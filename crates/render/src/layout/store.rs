//! The image store: every raster a document draws (file images, remote images, inline and display
//! math, Mermaid diagrams), keyed by what the pixels depend on rather than by where the image sits
//! in one layout. A relayout for a new width, a zoom, a `<details>` toggle, a reload or a tab
//! switch therefore finds what it drew before; only a key that really changed (a file edited on
//! disk, a formula at a new pixel size or colour, a diagram at a new scale or theme) costs a decode
//! or an engine job.
//!
//! Entries are interned: a key gets a small `u32` id that paints carry, so a cached paint list
//! needs no key comparison to draw. Ids are never reused. Decoded pixels are capped at
//! [`IMAGE_CACHE_BYTES`](mdview_core::limits::IMAGE_CACHE_BYTES) with least-recently-used
//! eviction; the key stays, so an evicted local image, formula or diagram loads again on its next
//! use. Nothing the current frame touched is evicted.

use crate::engine;
use crate::image::{self, DecodedImage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

/// What one raster depends on.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) enum ImageKey {
    /// A local file: its resolved path, length and modification time (so an edited file loads
    /// again), and for an SVG the device scale it was rasterized at (`f32` bits; 0 for the
    /// in-process formats, which do not depend on the scale).
    File {
        path: PathBuf,
        len: u64,
        modified: Option<SystemTime>,
        scale: u32,
    },
    /// An `http`/`https` image, by its URL as written. The renderer never fetches; a shell does,
    /// and hands the result to `DocumentRenderer::image_arrived`.
    Remote(String),
    /// A formula at a pixel size (`f32` bits) and RGB colour, inline or display.
    Math {
        source: String,
        size: u32,
        color: [u8; 3],
        display: bool,
    },
    /// A diagram at a raster scale (`f32` bits) in one theme.
    Mermaid {
        source: String,
        scale: u32,
        dark: bool,
    },
}

/// Where an entry stands.
pub(super) enum Slot {
    Ready(Rc<DecodedImage>),
    /// Could not be loaded or rendered; draws the placeholder until its key changes.
    Failed,
    /// A remote image the shell is fetching.
    Pending,
    /// A remote image the shell will not fetch (not permitted).
    Blocked,
    /// Pixels dropped by the byte cap; the key can load them again.
    Evicted,
}

/// What a lookup found, for the layout.
pub(super) enum Found {
    /// The raster and its density: raster pixels per logical pixel (1 for bitmaps, the
    /// rasterization scale for an SVG).
    Ready(Rc<DecodedImage>, f32),
    Failed,
    Pending,
    Blocked,
    /// Not in the store and not loaded, because this pass may not decode or run engine jobs (or,
    /// for a remote image, because nobody has said what to do with it yet).
    Unknown,
}

struct Entry {
    key: ImageKey,
    slot: Slot,
    density: f32,
    /// The frame that last used the entry, for the LRU order.
    used: u64,
}

/// Decodes and engine jobs since the renderer was made, for tests and traces.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageStats {
    /// Local image files read and decoded (SVG files included).
    pub decodes: u64,
    /// Jobs sent to `mdview-engine`: SVG files, formulas and diagrams.
    pub engine_jobs: u64,
    /// Decoded bytes held now.
    pub bytes: u64,
    /// Entries known, with or without pixels.
    pub entries: usize,
}

pub(super) struct ImageStore {
    ids: HashMap<ImageKey, u32>,
    entries: Vec<Entry>,
    /// Files learned to be SVG, by identity, so the next lookup asks for the scaled key directly
    /// instead of reading the file to find out.
    vector: HashMap<(PathBuf, u64, Option<SystemTime>), bool>,
    bytes: u64,
    cap: u64,
    clock: u64,
    stats: ImageStats,
    /// Remote URLs looked up while nobody had decided about them, oldest first, without repeats.
    remote_new: Vec<String>,
}

impl ImageStore {
    pub(super) fn new(cap: u64) -> ImageStore {
        ImageStore {
            ids: HashMap::new(),
            entries: Vec::new(),
            vector: HashMap::new(),
            bytes: 0,
            cap,
            clock: 1,
            stats: ImageStats::default(),
            remote_new: Vec::new(),
        }
    }

    pub(super) fn stats(&self) -> ImageStats {
        ImageStats {
            bytes: self.bytes,
            entries: self.entries.len(),
            ..self.stats
        }
    }

    /// Starts a frame: what it uses from here on is protected from eviction until the next one.
    pub(super) fn begin_frame(&mut self) {
        self.clock += 1;
    }

    fn intern(&mut self, key: ImageKey) -> u32 {
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.entries.len() as u32;
        self.entries.push(Entry {
            key: key.clone(),
            slot: Slot::Evicted,
            density: 1.0,
            used: 0,
        });
        self.ids.insert(key, id);
        id
    }

    fn found(&mut self, id: u32) -> Found {
        let clock = self.clock;
        let entry = &mut self.entries[id as usize];
        entry.used = clock;
        match &entry.slot {
            Slot::Ready(image) => Found::Ready(image.clone(), entry.density),
            Slot::Failed => Found::Failed,
            Slot::Pending => Found::Pending,
            Slot::Blocked => Found::Blocked,
            Slot::Evicted => Found::Unknown,
        }
    }

    /// Looks `key` up, loading it with `load` when it has no pixels and `allow` says this pass may.
    fn lookup(
        &mut self,
        key: ImageKey,
        allow: bool,
        load: impl FnOnce(&mut ImageStats) -> (Option<DecodedImage>, f32),
    ) -> (u32, Found) {
        let id = self.intern(key);
        if !matches!(self.entries[id as usize].slot, Slot::Evicted) || !allow {
            return (id, self.found(id));
        }
        let (image, density) = load(&mut self.stats);
        self.put(id, image, density);
        (id, self.found(id))
    }

    fn put(&mut self, id: u32, image: Option<DecodedImage>, density: f32) {
        let slot = match image {
            Some(image) => Slot::Ready(Rc::new(image)),
            None => Slot::Failed,
        };
        self.replace(id, slot, density);
    }

    /// Swaps an entry's slot, keeping the byte count, and evicts if that went over the cap.
    fn replace(&mut self, id: u32, slot: Slot, density: f32) {
        if let Slot::Ready(image) = &slot {
            self.bytes += image.rgba.len() as u64;
        }
        let clock = self.clock;
        let entry = &mut self.entries[id as usize];
        entry.density = density;
        entry.used = clock;
        if let Slot::Ready(old) = std::mem::replace(&mut entry.slot, slot) {
            self.bytes -= old.rgba.len() as u64;
        }
        self.evict();
    }

    /// Drops the least recently used pixels until the store is back under three quarters of the
    /// cap, so a full store does not sort its entries on every insert. Entries the current frame
    /// used stay, even past the cap.
    fn evict(&mut self) {
        if self.bytes <= self.cap {
            return;
        }
        let mut ready: Vec<(u64, u32)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.slot, Slot::Ready(_)) && e.used < self.clock)
            .map(|(i, e)| (e.used, i as u32))
            .collect();
        ready.sort_unstable();
        let goal = self.cap / 4 * 3;
        for (_, id) in ready {
            if self.bytes <= goal {
                break;
            }
            let entry = &mut self.entries[id as usize];
            if let Slot::Ready(image) = std::mem::replace(&mut entry.slot, Slot::Evicted) {
                self.bytes -= image.rgba.len() as u64;
            }
            // A remote image cannot be loaded again here: it goes back to the shell's queue.
            if let ImageKey::Remote(url) = &entry.key
                && !self.remote_new.contains(url)
            {
                self.remote_new.push(url.clone());
            }
        }
    }

    /// The pixels for a paint. Reloads a local image, formula or diagram whose pixels were
    /// evicted; a remote one draws nothing until the shell delivers it again.
    pub(super) fn pixels(&mut self, id: u32) -> Option<Rc<DecodedImage>> {
        let key = self.entries.get(id as usize)?.key.clone();
        let reload = matches!(self.entries[id as usize].slot, Slot::Evicted)
            && !matches!(key, ImageKey::Remote(_));
        if reload {
            let (image, density) = load_key(&key, &mut self.stats);
            self.put(id, image, density);
        }
        match self.found(id) {
            Found::Ready(image, _) => Some(image),
            _ => None,
        }
    }

    /// A local file at a device `scale` (which only an SVG's raster depends on).
    pub(super) fn file(&mut self, path: &Path, scale: f32, allow: bool) -> (u32, Found) {
        let Ok(meta) = std::fs::metadata(path) else {
            // Missing: remembered as failed under an identity a new file will not match.
            let key = ImageKey::File {
                path: path.to_path_buf(),
                len: u64::MAX,
                modified: None,
                scale: 0,
            };
            let id = self.intern(key);
            if matches!(self.entries[id as usize].slot, Slot::Evicted) {
                self.entries[id as usize].slot = Slot::Failed;
            }
            return (id, self.found(id));
        };
        let identity = (path.to_path_buf(), meta.len(), meta.modified().ok());
        let key_for = |vector: bool| ImageKey::File {
            path: identity.0.clone(),
            len: identity.1,
            modified: identity.2,
            scale: if vector { scale.to_bits() } else { 0 },
        };
        if let Some(&vector) = self.vector.get(&identity) {
            let key = key_for(vector);
            return self.lookup(key, allow, |stats| load_file(path, scale, stats).0);
        }
        if !allow {
            return (u32::MAX, Found::Unknown);
        }
        let ((image, density), vector) = load_file(path, scale, &mut self.stats);
        let key = key_for(vector);
        self.vector.insert(identity, vector);
        let id = self.intern(key);
        self.put(id, image, density);
        (id, self.found(id))
    }

    /// A remote image: never loaded here. One nobody has decided about is queued for the shell
    /// when `ask` (a pass that shows it; estimates and refinement only look).
    pub(super) fn remote(&mut self, url: &str, ask: bool) -> (u32, Found) {
        let id = self.intern(ImageKey::Remote(url.to_string()));
        let found = self.found(id);
        if ask && matches!(found, Found::Unknown) && !self.remote_new.iter().any(|u| u == url) {
            self.remote_new.push(url.to_string());
        }
        (id, found)
    }

    pub(super) fn math(
        &mut self,
        source: &str,
        size: f32,
        color: [u8; 3],
        display: bool,
        allow: bool,
    ) -> (u32, Found) {
        let key = ImageKey::Math {
            source: source.to_string(),
            size: size.to_bits(),
            color,
            display,
        };
        self.lookup(key, allow, |stats| {
            stats.engine_jobs += 1;
            (engine::render_math(source, display, size, color), 1.0)
        })
    }

    pub(super) fn mermaid(
        &mut self,
        source: &str,
        scale: f32,
        dark: bool,
        allow: bool,
    ) -> (u32, Found) {
        let key = ImageKey::Mermaid {
            source: source.to_string(),
            scale: scale.to_bits(),
            dark,
        };
        self.lookup(key, allow, |stats| {
            stats.engine_jobs += 1;
            (engine::render_mermaid(source, scale, dark), 1.0)
        })
    }

    /// Records what the shell decided or delivered for a remote URL. Returns the entry's id and
    /// whether anything changed.
    pub(super) fn set_remote(&mut self, url: &str, slot: Slot) -> (u32, bool) {
        let id = self.intern(ImageKey::Remote(url.to_string()));
        self.remote_new.retain(|u| u != url);
        let same = matches!(
            (&self.entries[id as usize].slot, &slot),
            (Slot::Failed, Slot::Failed)
                | (Slot::Pending, Slot::Pending)
                | (Slot::Blocked, Slot::Blocked)
        );
        if same {
            return (id, false);
        }
        self.replace(id, slot, 1.0);
        (id, true)
    }

    /// Remote URLs met since the last call that nobody has decided about.
    pub(super) fn take_remote(&mut self) -> Vec<String> {
        std::mem::take(&mut self.remote_new)
    }
}

/// Reads and decodes a file; an SVG is rasterized by the engine at `scale`. Also says whether it
/// was an SVG.
fn load_file(
    path: &Path,
    scale: f32,
    stats: &mut ImageStats,
) -> ((Option<DecodedImage>, f32), bool) {
    stats.decodes += 1;
    let Some(bytes) = image::read(path) else {
        return ((None, 1.0), false);
    };
    if image::looks_like_svg(&bytes) {
        stats.engine_jobs += 1;
        ((engine::render_svg_at(&bytes, scale), scale), true)
    } else {
        ((image::decode(&bytes), 1.0), false)
    }
}

/// Loads a key again after eviction.
fn load_key(key: &ImageKey, stats: &mut ImageStats) -> (Option<DecodedImage>, f32) {
    match key {
        ImageKey::File { path, scale, .. } => {
            let scale = if *scale == 0 {
                1.0
            } else {
                f32::from_bits(*scale)
            };
            load_file(path, scale, stats).0
        }
        ImageKey::Remote(_) => (None, 1.0),
        ImageKey::Math {
            source,
            size,
            color,
            display,
        } => {
            stats.engine_jobs += 1;
            (
                engine::render_math(source, *display, f32::from_bits(*size), *color),
                1.0,
            )
        }
        ImageKey::Mermaid {
            source,
            scale,
            dark,
        } => {
            stats.engine_jobs += 1;
            (
                engine::render_mermaid(source, f32::from_bits(*scale), *dark),
                1.0,
            )
        }
    }
}
