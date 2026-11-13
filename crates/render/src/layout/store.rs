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
//!
//! An animated GIF or WebP is an entry whose pixels change: its slot holds the frame on screen,
//! and an [`Anim`] beside it holds the frame decoded ahead and when it is due. The decoding is
//! `crate::anim`'s, on its worker thread; [`ImageStore::advance`] swaps frames in at their times.
//! Nothing here runs unless the shell turned animation on and asks.

use crate::anim::{self, Job, Reply, Waker, Worker};
use crate::engine;
use crate::image::{self, DecodedImage};
use mdview_core::limits::{ANIMATION_BYTES, MAX_ANIMATION_KEPT_BYTES, MAX_ANIMATION_PIXELS};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

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
    /// What playing this entry would hold beyond its slot (see `ANIMATION_BYTES`): set for a GIF
    /// or animated WebP small enough to play, 0 for everything else and once it is known to be
    /// still or finished.
    plays: u64,
    anim: Option<Anim>,
}

/// The moving part of an entry that plays.
struct Anim {
    /// Its player on the worker; 0 while it has none (given up for room, or not started yet).
    token: u64,
    /// The frame decoded ahead.
    next: Option<anim::Step>,
    /// When the frame on screen went up; `None` while the image is off screen, so its clock
    /// stands still there.
    shown: Option<Instant>,
    /// The `advance` call that last found it on screen.
    seen: u64,
    /// The box, in device px, the image is drawn into when that is below half its size: frames
    /// are then asked for at this size.
    size: Option<(u32, u32)>,
    /// The frame on screen when it came at `size`. The slot then still holds an older full-size
    /// frame, which only layout looks at (for the image's dimensions).
    drawn: Option<Rc<DecodedImage>>,
    /// The frames of one loop at `size`, from the first on, each with how long the frame before
    /// it stays up. Collected only for an animation that loops for ever.
    kept: Vec<(Rc<DecodedImage>, Duration)>,
    /// `kept` is the whole loop and plays without a decoder; `cursor` is the frame shown.
    looping: bool,
    cursor: usize,
}

impl Anim {
    fn new(seen: u64) -> Anim {
        Anim {
            token: 0,
            next: None,
            shown: None,
            seen,
            size: None,
            drawn: None,
            kept: Vec::new(),
            looping: false,
            cursor: 0,
        }
    }

    fn kept_bytes(&self) -> u64 {
        self.kept
            .iter()
            .map(|(image, _)| image.rgba.len() as u64)
            .sum()
    }
}

/// Everything animation needs that is not one entry's.
struct Playback {
    enabled: bool,
    waker: Option<Waker>,
    worker: Option<Worker>,
    /// Live players' tokens and their entries.
    tokens: HashMap<u64, u32>,
    last_token: u64,
    /// Entries that keep frames of their loop.
    kept_ids: Vec<u32>,
    /// What the live players and the kept frames hold, against `budget`.
    bytes: u64,
    budget: u64,
    tick: u64,
    /// Entries with an `Anim`, playing or waiting for room.
    count: usize,
    frames: u64,
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
    /// Animations with a decoder running now.
    pub playing: usize,
    /// Animation frames put on screen so far, after the first of each image.
    pub frames: u64,
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
    play: Playback,
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
            play: Playback {
                enabled: false,
                waker: None,
                worker: None,
                tokens: HashMap::new(),
                last_token: 0,
                kept_ids: Vec::new(),
                bytes: 0,
                budget: ANIMATION_BYTES,
                tick: 1,
                count: 0,
                frames: 0,
            },
        }
    }

    pub(super) fn stats(&self) -> ImageStats {
        ImageStats {
            bytes: self.bytes,
            entries: self.entries.len(),
            playing: self.play.tokens.len() + self.looping(),
            frames: self.play.frames,
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
            plays: 0,
            anim: None,
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
        // New content: whatever was playing here is over.
        entry.plays = 0;
        if let Some(anim) = entry.anim.take() {
            self.release(id, anim);
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
            entry.plays = 0;
            if let Some(anim) = entry.anim.take() {
                self.release(id, anim);
            }
            let entry = &mut self.entries[id as usize];
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
            if let ImageKey::File { path, scale, .. } = &key {
                let scale = if *scale == 0 {
                    1.0
                } else {
                    f32::from_bits(*scale)
                };
                let loaded = load_file(path, scale, &mut self.stats);
                self.put_loaded(id, loaded);
            } else {
                let (image, density) = load_key(&key, &mut self.stats);
                self.put(id, image, density);
            }
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
            let id = self.intern(key_for(vector));
            if allow && matches!(self.entries[id as usize].slot, Slot::Evicted) {
                let loaded = load_file(path, scale, &mut self.stats);
                self.put_loaded(id, loaded);
            }
            return (id, self.found(id));
        }
        if !allow {
            return (u32::MAX, Found::Unknown);
        }
        let loaded = load_file(path, scale, &mut self.stats);
        let key = key_for(loaded.vector);
        self.vector.insert(identity, loaded.vector);
        let id = self.intern(key);
        self.put_loaded(id, loaded);
        (id, self.found(id))
    }

    fn put_loaded(&mut self, id: u32, loaded: Loaded) {
        self.put(id, loaded.image, loaded.density);
        if let Some(bytes) = loaded.animates {
            self.offer(id, Cow::Owned(bytes));
        }
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
    pub(super) fn set_remote(&mut self, url: &str, slot: Slot, density: f32) -> (u32, bool) {
        let id = self.intern(ImageKey::Remote(url.to_string()));
        self.remote_new.retain(|u| u != url);
        let same = matches!(
            (&self.entries[id as usize].slot, &slot),
            (Slot::Failed, Slot::Failed)
                | (Slot::Pending, Slot::Pending)
                | (Slot::Blocked, Slot::Blocked)
        );
        // A fetch under way for an image that is showing (its animation is starting again) leaves
        // the pixels up until the bytes arrive.
        let waits = matches!(slot, Slot::Pending)
            && matches!(self.entries[id as usize].slot, Slot::Ready(_));
        if same || waits {
            return (id, false);
        }
        self.replace(id, slot, density);
        (id, true)
    }

    pub(super) fn count_engine_job(&mut self) {
        self.stats.engine_jobs += 1;
    }

    /// Remote URLs met since the last call that nobody has decided about.
    pub(super) fn take_remote(&mut self) -> Vec<String> {
        std::mem::take(&mut self.remote_new)
    }
}

/// Animation: which entries play, their players on the worker, and the swap of frames.
impl ImageStore {
    /// Turns animation on or off (off until a shell asks). Turning it off drops every player and
    /// puts local images back on their first frame (they load again when next drawn); a remote
    /// one keeps the frame it shows. Turning it on lets what is already loaded start when it is
    /// next on screen. True when the setting changed.
    pub(super) fn set_animation(&mut self, on: bool) -> bool {
        if self.play.enabled == on {
            return false;
        }
        self.play.enabled = on;
        for id in 0..self.entries.len() {
            let entry = &mut self.entries[id];
            if entry.plays == 0 || !matches!(entry.slot, Slot::Ready(_)) {
                continue;
            }
            if on {
                if entry.anim.is_none() {
                    entry.anim = Some(Anim::new(0));
                    self.play.count += 1;
                }
                continue;
            }
            let Some(anim) = entry.anim.take() else {
                continue;
            };
            if matches!(entry.key, ImageKey::File { .. })
                && let Slot::Ready(image) = std::mem::replace(&mut entry.slot, Slot::Evicted)
            {
                self.bytes -= image.rgba.len() as u64;
            }
            self.release(id as u32, anim);
        }
        true
    }

    /// Animations playing from kept frames, without a decoder.
    fn looping(&self) -> usize {
        self.play
            .kept_ids
            .iter()
            .filter_map(|&id| self.entries[id as usize].anim.as_ref())
            .filter(|anim| anim.looping)
            .count()
    }

    /// What the worker calls after each frame it has ready. Set before the first image loads.
    pub(super) fn set_waker(&mut self, waker: Waker) {
        self.play.waker = Some(waker);
    }

    #[cfg(test)]
    pub(super) fn set_animation_budget(&mut self, bytes: u64) {
        self.play.budget = bytes;
    }

    /// Players on the worker now.
    #[cfg(test)]
    pub(super) fn decoders(&self) -> usize {
        self.play.tokens.len()
    }

    /// Whether any entry plays or waits to, so a caller with nothing animated does no work.
    pub(super) fn animating(&self) -> bool {
        self.play.enabled && self.play.count > 0
    }

    /// Whether entry `id` plays or waits to.
    pub(super) fn animated(&self, id: u32) -> bool {
        self.entries
            .get(id as usize)
            .is_some_and(|e| e.anim.is_some())
    }

    fn send(&mut self, job: Job) -> bool {
        if self.play.worker.is_none() {
            self.play.worker = Worker::spawn(self.play.waker.clone());
        }
        match &self.play.worker {
            Some(worker) => worker.jobs.send(job).is_ok(),
            None => false,
        }
    }

    /// Gives back what an `Anim` taken off entry `id` held.
    fn release(&mut self, id: u32, anim: Anim) {
        self.play.count -= 1;
        if anim.token != 0 {
            self.play.tokens.remove(&anim.token);
            self.send(Job::Drop(anim.token));
        }
        if !anim.kept.is_empty() {
            self.play.kept_ids.retain(|&other| other != id);
        }
        self.recount();
    }

    /// The bytes live players and kept frames hold, from their entries.
    fn recount(&mut self) {
        let players: u64 = self
            .play
            .tokens
            .values()
            .map(|&id| self.entries[id as usize].plays)
            .sum();
        let kept: u64 = self
            .play
            .kept_ids
            .iter()
            .filter_map(|&id| self.entries[id as usize].anim.as_ref())
            .map(Anim::kept_bytes)
            .sum();
        self.play.bytes = players + kept;
    }

    /// Offers the bytes entry `id` was just decoded from: if they may animate and the canvas is
    /// within the limit, the entry gets a player (now, when there is room and animation is on).
    pub(super) fn offer(&mut self, id: u32, bytes: Cow<'_, [u8]>) {
        let Some(entry) = self.entries.get_mut(id as usize) else {
            return;
        };
        let Slot::Ready(image) = &entry.slot else {
            return;
        };
        let pixels = u64::from(image.width) * u64::from(image.height);
        if pixels > MAX_ANIMATION_PIXELS || !anim::candidate(&bytes) {
            return;
        }
        // The file, the canvas, its saved copy and the frame in flight (4 bytes a pixel each).
        entry.plays = bytes.len() as u64 + pixels * 12;
        if !self.play.enabled || entry.anim.is_some() {
            return;
        }
        entry.anim = Some(Anim::new(self.play.tick));
        self.play.count += 1;
        if self.make_room(id) {
            self.start(id, bytes.into_owned());
        }
    }

    /// Starts a player for an entry that has an `Anim` without one.
    fn start(&mut self, id: u32, bytes: Vec<u8>) {
        self.play.last_token += 1;
        let token = self.play.last_token;
        let size = self.entries[id as usize].anim.as_ref().and_then(|a| a.size);
        if !self.send(Job::Start { token, bytes, size }) {
            // No worker thread: the image stays still.
            self.still(id);
            return;
        }
        if let Some(anim) = self.entries[id as usize].anim.as_mut() {
            anim.token = token;
            anim.next = None;
            anim.shown = None;
        }
        self.play.tokens.insert(token, id);
        self.recount();
    }

    /// Makes room in the budget for entry `id`'s player by taking players and kept frames from
    /// animations that were not on screen at the last look, longest unseen first. False when it
    /// still does not fit.
    fn make_room(&mut self, id: u32) -> bool {
        let cost = self.entries[id as usize].plays;
        if cost > self.play.budget {
            return false;
        }
        while self.play.bytes + cost > self.play.budget {
            let tick = self.play.tick;
            let oldest = self
                .play
                .tokens
                .values()
                .chain(&self.play.kept_ids)
                .filter_map(|&other| {
                    let seen = self.entries[other as usize].anim.as_ref()?.seen;
                    (seen < tick && other != id).then_some((seen, other))
                })
                .min();
            let Some((_, other)) = oldest else {
                return false;
            };
            self.park(other);
        }
        true
    }

    /// Takes an entry's player and kept frames away; the entry keeps the frame it shows and
    /// starts over when next shown.
    fn park(&mut self, id: u32) {
        let Some(anim) = self.entries[id as usize].anim.as_mut() else {
            return;
        };
        let token = std::mem::take(&mut anim.token);
        anim.next = None;
        anim.shown = None;
        anim.kept.clear();
        anim.looping = false;
        self.play.tokens.remove(&token);
        self.play.kept_ids.retain(|&other| other != id);
        if token != 0 {
            self.send(Job::Drop(token));
        }
        self.recount();
    }

    /// Gives a shown entry without a player one again: a file is read again, a remote image goes
    /// back to the shell's queue (its arrival offers the bytes).
    fn revive(&mut self, id: u32) {
        if !self.make_room(id) {
            return;
        }
        let key = self.entries[id as usize].key.clone();
        match key {
            ImageKey::File { path, .. } => match image::read(&path) {
                Some(bytes) if anim::candidate(&bytes) => self.start(id, bytes),
                _ => self.still(id),
            },
            ImageKey::Remote(url) => {
                self.still(id);
                if !self.remote_new.contains(&url) {
                    self.remote_new.push(url);
                }
            }
            _ => self.still(id),
        }
    }

    /// Drops an entry's `Anim`. The frame it shows stays: one drawn from a scaled copy has no
    /// full-size pixels of its own, so the entry falls back to the last full frame it held.
    fn still(&mut self, id: u32) {
        if let Some(anim) = self.entries[id as usize].anim.take() {
            self.release(id, anim);
        }
    }

    /// The pixels to draw entry `id` with into a box of `w` by `h` device px. For an animation
    /// shown below half size this is the current frame already scaled to that box (the worker
    /// scales, once per frame, so a tick costs a copy and not the area filter); the box is
    /// remembered so the next frames come at its size. Everything else is [`pixels`](Self::pixels).
    /// True with pixels that are opaque and exactly the size of the box.
    pub(super) fn pixels_at(
        &mut self,
        id: u32,
        w: f32,
        h: f32,
    ) -> Option<(Rc<DecodedImage>, bool)> {
        let entry = self.entries.get_mut(id as usize)?;
        if let (Some(anim), Slot::Ready(full)) = (entry.anim.as_mut(), &entry.slot) {
            let (dw, dh) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
            let small = dw * 2 < full.width || dh * 2 < full.height;
            let size = small.then_some((dw, dh));
            if anim.size != size {
                // Another zoom or width: frames kept for the old box are of no use.
                anim.size = size;
                let had = !anim.kept.is_empty();
                anim.kept.clear();
                anim.looping = false;
                if had {
                    self.play.kept_ids.retain(|&other| other != id);
                    self.recount();
                }
            }
            let anim = self.entries[id as usize].anim.as_ref()?;
            if let Some(drawn) = &anim.drawn
                && Some((drawn.width, drawn.height)) == size
            {
                return Some((drawn.clone(), true));
            }
        }
        self.pixels(id).map(|image| (image, false))
    }

    /// Takes what the worker has ready, then puts up every frame that is due among the entries
    /// in `visible` (the animated images on screen; only they move). Returns the entries whose
    /// pixels changed and the time the next frame in hand is due; a frame still being decoded has
    /// no time, the worker's wake announces it.
    pub(super) fn advance(&mut self, now: Instant, visible: &[u32]) -> (Vec<u32>, Option<Instant>) {
        let replies: Vec<Reply> = match &self.play.worker {
            Some(worker) => worker.replies.try_iter().collect(),
            None => Vec::new(),
        };
        for reply in replies {
            match reply {
                Reply::Frame(token, step) => {
                    let Some(&id) = self.play.tokens.get(&token) else {
                        continue;
                    };
                    if let Some(anim) = self.entries[id as usize].anim.as_mut() {
                        anim.next = Some(step);
                    }
                }
                Reply::End(token) => {
                    // Still after all, or its last loop is over: the frame shown stays.
                    if let Some(&id) = self.play.tokens.get(&token) {
                        self.entries[id as usize].plays = 0;
                        self.finish(id);
                    }
                }
            }
        }
        self.play.tick += 1;
        let tick = self.play.tick;
        for &id in visible {
            if let Some(anim) = self
                .entries
                .get_mut(id as usize)
                .and_then(|e| e.anim.as_mut())
            {
                anim.seen = tick;
            }
        }
        // Off screen, the clock stands still.
        let playing: Vec<u32> = self
            .play
            .tokens
            .values()
            .chain(&self.play.kept_ids)
            .copied()
            .collect();
        for id in playing {
            if let Some(anim) = self.entries[id as usize].anim.as_mut()
                && anim.seen != tick
            {
                anim.shown = None;
            }
        }
        let mut changed = Vec::new();
        let mut next: Option<Instant> = None;
        let mut soonest = |due: Instant| next = Some(next.map_or(due, |n: Instant| n.min(due)));
        for &id in visible {
            let Some(anim) = self
                .entries
                .get_mut(id as usize)
                .and_then(|e| e.anim.as_mut())
            else {
                continue;
            };
            if anim.looping {
                // Every frame of the loop is kept at the size shown: no decoder, only a clock.
                let shown = *anim.shown.get_or_insert(now);
                let count = anim.kept.len();
                let after = anim.kept[(anim.cursor + 1) % count].1;
                let due = shown + after;
                if now < due {
                    soonest(due);
                    continue;
                }
                anim.cursor = (anim.cursor + 1) % count;
                anim.drawn = Some(anim.kept[anim.cursor].0.clone());
                let from = on_time(now, due, after);
                anim.shown = Some(from);
                soonest(from + anim.kept[(anim.cursor + 1) % count].1);
                self.play.frames += 1;
                changed.push(id);
                continue;
            }
            if anim.token == 0 {
                self.revive(id);
                continue;
            }
            let shown = *anim.shown.get_or_insert(now);
            let Some(after) = anim.next.as_ref().map(|step| step.after) else {
                continue;
            };
            let due = shown + after;
            if now < due {
                soonest(due);
                continue;
            }
            let Some(step) = anim.next.take() else {
                continue;
            };
            anim.shown = Some(on_time(now, due, after));
            self.show(id, step);
            self.play.frames += 1;
            changed.push(id);
        }
        (changed, next)
    }

    /// Puts a decoded frame up on entry `id` and asks for the one after it (or, once a whole
    /// loop is kept at the size shown, lets the decoder go).
    fn show(&mut self, id: u32, step: anim::Step) {
        let entry = &mut self.entries[id as usize];
        let Some(anim) = entry.anim.as_mut() else {
            return;
        };
        let image = Rc::new(DecodedImage {
            width: step.width,
            height: step.height,
            baseline: step.height,
            rgba: step.rgba,
        });
        let frame_bytes = image.rgba.len() as u64;
        let had = !anim.kept.is_empty();
        let mut round = false;
        if !step.scaled {
            // A full-size frame is the entry's own pixels, as a still image's are.
            anim.drawn = None;
            anim.kept.clear();
            self.bytes += frame_bytes;
            if let Slot::Ready(old) = std::mem::replace(&mut entry.slot, Slot::Ready(image)) {
                self.bytes -= old.rgba.len() as u64;
            }
        } else {
            // A frame at the size shown is kept, from the first of a loop on, while the loop
            // fits; when the first comes round again the whole loop is in hand.
            let fits = anim.size == Some((step.width, step.height))
                && step.forever
                && anim.kept_bytes() + frame_bytes <= MAX_ANIMATION_KEPT_BYTES
                && self.play.bytes + frame_bytes <= self.play.budget;
            if !fits {
                anim.kept.clear();
            } else if step.index == 0 && !anim.kept.is_empty() {
                round = true;
            } else if step.index as usize == anim.kept.len() {
                anim.kept.push((image.clone(), step.after));
            } else {
                anim.kept.clear();
            }
            anim.drawn = Some(image);
        }
        let (token, size) = (anim.token, anim.size);
        let has = !anim.kept.is_empty();
        if round {
            anim.looping = true;
            anim.cursor = 0;
            anim.token = 0;
            anim.next = None;
            self.play.tokens.remove(&token);
            self.send(Job::Drop(token));
        } else {
            self.send(Job::Next(token, size));
        }
        if has && !had {
            self.play.kept_ids.push(id);
        } else if had && !has {
            self.play.kept_ids.retain(|&other| other != id);
        }
        self.recount();
    }

    /// Ends entry `id`'s animation on the frame it shows.
    fn finish(&mut self, id: u32) {
        let entry = &mut self.entries[id as usize];
        let Some(mut anim) = entry.anim.take() else {
            return;
        };
        // A frame drawn from a scaled copy is not the entry's own pixels; the last frame the
        // player sends comes full size, so only an animation cut short can end on a scaled one,
        // and it then shows the last full frame it held.
        anim.drawn = None;
        self.release(id, anim);
    }
}

/// When a frame put up at `now` counts as shown: when it was due, so delays do not drift, unless
/// it is more than its own delay late (a slow decode, a hidden window), then now.
fn on_time(now: Instant, due: Instant, after: Duration) -> Instant {
    if now.duration_since(due) > after {
        now
    } else {
        due
    }
}

/// What reading a file gave.
struct Loaded {
    image: Option<DecodedImage>,
    density: f32,
    /// An SVG, rasterized by the engine.
    vector: bool,
    /// The file's bytes when it decoded and is of a kind that may animate.
    animates: Option<Vec<u8>>,
}

/// Reads and decodes a file; an SVG is rasterized by the engine at `scale`.
fn load_file(path: &Path, scale: f32, stats: &mut ImageStats) -> Loaded {
    stats.decodes += 1;
    let Some(bytes) = image::read(path) else {
        return Loaded {
            image: None,
            density: 1.0,
            vector: false,
            animates: None,
        };
    };
    if image::looks_like_svg(&bytes) {
        stats.engine_jobs += 1;
        Loaded {
            image: engine::render_svg_at(&bytes, scale),
            density: scale,
            vector: true,
            animates: None,
        }
    } else {
        let image = image::decode(&bytes);
        let animates = (image.is_some() && anim::candidate(&bytes)).then_some(bytes);
        Loaded {
            image,
            density: 1.0,
            vector: false,
            animates,
        }
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
            let loaded = load_file(path, scale, stats);
            (loaded.image, loaded.density)
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
