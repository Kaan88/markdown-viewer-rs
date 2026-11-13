//! Animated GIF and WebP: the frames after the first.
//!
//! A [`Player`] walks one file forward, a frame at a time, keeping only the composited canvas (and,
//! for a GIF frame disposed "to previous", the rectangle it covered). It never holds a second
//! decoded frame, so what an animation costs does not grow with its length; a loop starts the
//! decoder over from the first byte. The limits in `mdview_core::limits` decide what plays at all:
//! a file with one frame, more than `MAX_ANIMATION_FRAMES`, or a canvas over
//! `MAX_ANIMATION_PIXELS` has no player and keeps the first frame the image decoder drew.
//!
//! Players live on one worker thread ([`Worker`]), started by the first image that may animate.
//! It sleeps in `recv` until it is asked for a frame, decodes exactly that one, and sleeps again,
//! so nothing spins and the window's thread never decodes more than the first frame.

use mdview_core::limits::{
    ANIMATION_DEFAULT_DELAY_MS, ANIMATION_FAST_DELAY_MS, MAX_ANIMATION_FRAMES, MAX_ANIMATION_PIXELS,
};
use std::collections::HashMap;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

type Bytes = Arc<[u8]>;

/// One composited frame, straight-alpha RGBA the size of the canvas.
pub(crate) struct Step {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// How long the frame before this one stays up.
    pub after: Duration,
    /// Its place in the loop, from 0.
    pub index: u32,
    /// The animation loops for ever.
    pub forever: bool,
    /// `rgba` is not the canvas but the canvas scaled down to the size asked for.
    pub scaled: bool,
}

/// Scales a frame down to the box it is shown in, `size`, when that is below half the canvas on
/// an axis: the case where drawing takes the area filter, which is too slow to run on the
/// window's thread for every frame of a large animation. The scaling is the bitmap's own, so the
/// result is what drawing the full frame into that box gives. Left alone: frames with any
/// transparency (the bitmap can only draw onto an opaque background) and animations that end
/// (their last frame has to stay as the image's own full-size pixels).
fn fit(mut step: Step, size: Option<(u32, u32)>) -> Step {
    let Some((width, height)) = size else {
        return step;
    };
    let smaller = width * 2 < step.width || height * 2 < step.height;
    if !step.forever
        || !smaller
        || width == 0
        || height == 0
        || width > step.width
        || height > step.height
        || step
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 255)
    {
        return step;
    }
    let mut bitmap = crate::bitmap::Bitmap::new(width, height);
    bitmap.image(
        &step.rgba,
        step.width,
        step.height,
        0.0,
        0.0,
        width as f32,
        height as f32,
    );
    // BGRA to straight RGBA, opaque.
    let mut rgba = bitmap.pixels;
    for pixel in rgba.as_chunks_mut::<4>().0.iter_mut() {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }
    step.rgba = rgba;
    step.width = width;
    step.height = height;
    step.scaled = true;
    step
}

/// Whether the bytes are of a kind that can hold more than one frame: any GIF, and a WebP whose
/// header sets the animation flag. Cheap; whether a GIF really has a second frame is the
/// player's to find out.
pub(crate) fn candidate(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return true;
    }
    bytes.len() > 20
        && &bytes[..4] == b"RIFF"
        && &bytes[8..16] == b"WEBPVP8X"
        && bytes[20] & 0x02 != 0
}

/// A file's frame delay as browsers play it: 10 ms or less means 100 ms.
fn delay(ms: u32) -> Duration {
    Duration::from_millis(u64::from(if ms <= ANIMATION_FAST_DELAY_MS {
        ANIMATION_DEFAULT_DELAY_MS
    } else {
        ms
    }))
}

fn fits(width: u32, height: u32) -> bool {
    width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_ANIMATION_PIXELS
}

pub(crate) enum Player {
    Gif(Box<GifPlayer>),
    Webp(Box<WebpPlayer>),
}

impl Player {
    /// A player standing on the file's first frame (which the caller already shows), or `None`
    /// when the file does not animate: one frame, over a limit, or not decodable.
    pub(crate) fn open(bytes: Vec<u8>) -> Option<Player> {
        let bytes: Bytes = Arc::from(bytes);
        if bytes.starts_with(b"GIF") {
            GifPlayer::open(bytes).map(|p| Player::Gif(Box::new(p)))
        } else {
            WebpPlayer::open(bytes).map(|p| Player::Webp(Box::new(p)))
        }
    }

    /// The next frame, looping as the file says. `None` once the last loop has ended (the frame
    /// shown then stays) or the file cannot be read further.
    pub(crate) fn next(&mut self) -> Option<Step> {
        match self {
            Player::Gif(p) => p.next(),
            Player::Webp(p) => p.next(),
        }
    }
}

/// A rectangle of the canvas, already clipped to it.
#[derive(Clone, Copy)]
struct Area {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

/// What a GIF frame asks to have done to the canvas before the next one is drawn.
enum Dispose {
    Keep,
    /// Back to transparent (browsers ignore the background colour).
    Clear(Area),
    /// Back to what was there before the frame.
    Restore(Area, Vec<u8>),
}

pub(crate) struct GifPlayer {
    bytes: Bytes,
    decoder: gif::Decoder<Cursor<Bytes>>,
    width: usize,
    height: usize,
    canvas: Vec<u8>,
    /// The frame being drawn, as palette indices, and its palette.
    indices: Vec<u8>,
    palette: Vec<u8>,
    dispose: Dispose,
    /// The delay of the frame drawn last.
    hold: Duration,
    /// Loops still to play after this one; `None` for ever.
    plays_left: Option<u32>,
    /// Frames drawn in this loop.
    drawn: u32,
}

fn gif_decoder(bytes: &Bytes) -> Option<gif::Decoder<Cursor<Bytes>>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(
        MAX_ANIMATION_PIXELS * 4,
    )?));
    options.read_info(Cursor::new(bytes.clone())).ok()
}

/// What a walk over a GIF's blocks finds, without decoding any pixels.
struct GifScan {
    frames: u32,
    repeat: gif::Repeat,
    screen: (u32, u32),
    /// The first frame's right and bottom edge, the canvas when the screen is 0 x 0.
    first: (u32, u32),
}

/// Counts the frames; `None` past `MAX_ANIMATION_FRAMES`. A file that breaks off part way counts
/// the frames whose headers were read.
fn scan_gif(bytes: &Bytes) -> Option<GifScan> {
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut decoder = options.read_info(Cursor::new(bytes.clone())).ok()?;
    let screen = (u32::from(decoder.width()), u32::from(decoder.height()));
    let (mut frames, mut first) = (0u32, (0, 0));
    while let Ok(Some(frame)) = decoder.next_frame_info() {
        if frames == 0 {
            first = (
                u32::from(frame.left) + u32::from(frame.width),
                u32::from(frame.top) + u32::from(frame.height),
            );
        }
        frames += 1;
        if frames > MAX_ANIMATION_FRAMES {
            return None;
        }
    }
    Some(GifScan {
        frames,
        repeat: decoder.repeat(),
        screen,
        first,
    })
}

impl GifPlayer {
    fn open(bytes: Bytes) -> Option<GifPlayer> {
        let scan = scan_gif(&bytes)?;
        if scan.frames < 2 {
            return None;
        }
        // The canvas the first-frame decoder used: the logical screen, or the first frame's
        // extent for the zero-sized screens that turn up in the wild.
        let width = if scan.screen.0 == 0 {
            scan.first.0
        } else {
            scan.screen.0
        };
        let height = if scan.screen.1 == 0 {
            scan.first.1
        } else {
            scan.screen.1
        };
        if !fits(width, height) {
            return None;
        }
        let (width, height) = (width as usize, height as usize);
        let mut player = GifPlayer {
            decoder: gif_decoder(&bytes)?,
            bytes,
            width,
            height,
            canvas: vec![0; width * height * 4],
            indices: Vec::new(),
            palette: Vec::new(),
            dispose: Dispose::Keep,
            hold: Duration::ZERO,
            // No Netscape block: once. A count of n there means n repeats after the first play.
            plays_left: match scan.repeat {
                gif::Repeat::Infinite => None,
                gif::Repeat::Finite(n) => Some(u32::from(n)),
            },
            drawn: 0,
        };
        if !player.draw() {
            return None;
        }
        player.drawn = 1;
        Some(player)
    }

    /// Disposes of the last frame and draws the next onto the canvas. False at the end of the
    /// file or at a frame that cannot be read.
    fn draw(&mut self) -> bool {
        let (width, height) = (self.width, self.height);
        match std::mem::replace(&mut self.dispose, Dispose::Keep) {
            Dispose::Keep => {}
            Dispose::Clear(area) => {
                for row in 0..area.height {
                    let at = ((area.top + row) * width + area.left) * 4;
                    self.canvas[at..at + area.width * 4].fill(0);
                }
            }
            Dispose::Restore(area, saved) => {
                for row in 0..area.height {
                    let at = ((area.top + row) * width + area.left) * 4;
                    let from = row * area.width * 4;
                    self.canvas[at..at + area.width * 4]
                        .copy_from_slice(&saved[from..from + area.width * 4]);
                }
            }
        }
        let Ok(Some(frame)) = self.decoder.next_frame_info() else {
            return false;
        };
        let (left, top) = (frame.left as usize, frame.top as usize);
        let (frame_w, frame_h) = (frame.width as usize, frame.height as usize);
        let (transparent, method) = (frame.transparent, frame.dispose);
        let hold = delay(u32::from(frame.delay) * 10);
        // A frame may be larger than the canvas (it is clipped below), but not without bound.
        if !fits(frame_w as u32, frame_h as u32) {
            return false;
        }
        let Ok(palette) = self.decoder.palette() else {
            return false;
        };
        self.palette.clear();
        self.palette.extend_from_slice(palette);
        if self.decoder.buffer_size() != frame_w * frame_h {
            return false;
        }
        self.indices.resize(frame_w * frame_h, 0);
        if self.decoder.read_into_buffer(&mut self.indices).is_err() {
            return false;
        }
        // The part of the frame inside the canvas; none when it starts past an edge.
        let area = Area {
            left: left.min(width),
            top: top.min(height),
            width: frame_w.min(width.saturating_sub(left)),
            height: frame_h.min(height.saturating_sub(top)),
        };
        let saved = matches!(method, gif::DisposalMethod::Previous).then(|| {
            let mut saved = Vec::with_capacity(area.width * area.height * 4);
            for row in 0..area.height {
                let at = ((area.top + row) * width + area.left) * 4;
                saved.extend_from_slice(&self.canvas[at..at + area.width * 4]);
            }
            saved
        });
        for row in 0..area.height {
            let line = &self.indices[row * frame_w..row * frame_w + area.width];
            let at = ((area.top + row) * width + area.left) * 4;
            let out = &mut self.canvas[at..at + area.width * 4];
            for (pixel, &index) in out.as_chunks_mut::<4>().0.iter_mut().zip(line) {
                if Some(index) == transparent {
                    continue;
                }
                // An index past the palette's end draws nothing.
                let entry = usize::from(index) * 3;
                if let Some(rgb) = self.palette.get(entry..entry + 3) {
                    pixel[..3].copy_from_slice(rgb);
                    pixel[3] = 255;
                }
            }
        }
        self.dispose = match (method, saved) {
            (gif::DisposalMethod::Background, _) => Dispose::Clear(area),
            (gif::DisposalMethod::Previous, Some(saved)) => Dispose::Restore(area, saved),
            _ => Dispose::Keep,
        };
        self.hold = hold;
        true
    }

    fn next(&mut self) -> Option<Step> {
        let after = self.hold;
        if !self.draw() {
            // The end of a loop. A loop that never reached a second frame is not an animation,
            // which also keeps a file that fails at once from being started over and over.
            if self.drawn < 2 {
                return None;
            }
            match self.plays_left {
                Some(0) => return None,
                Some(n) => self.plays_left = Some(n - 1),
                None => {}
            }
            self.decoder = gif_decoder(&self.bytes)?;
            self.canvas.fill(0);
            self.dispose = Dispose::Keep;
            self.drawn = 0;
            if !self.draw() {
                return None;
            }
        }
        self.drawn += 1;
        if self.drawn > MAX_ANIMATION_FRAMES {
            return None;
        }
        Some(Step {
            rgba: self.canvas.clone(),
            width: self.width as u32,
            height: self.height as u32,
            after,
            index: self.drawn - 1,
            forever: self.plays_left.is_none(),
            scaled: false,
        })
    }
}

pub(crate) struct WebpPlayer {
    bytes: Bytes,
    decoder: image_webp::WebPDecoder<Cursor<Bytes>>,
    /// The canvas as the decoder hands it out: RGBA, or RGB for a file without alpha.
    buffer: Vec<u8>,
    alpha: bool,
    width: u32,
    height: u32,
    hold: Duration,
    plays_left: Option<u32>,
    drawn: u32,
}

fn webp_decoder(bytes: &Bytes) -> Option<image_webp::WebPDecoder<Cursor<Bytes>>> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes.clone())).ok()?;
    decoder.set_memory_limit(MAX_ANIMATION_PIXELS as usize * 8);
    Some(decoder)
}

impl WebpPlayer {
    fn open(bytes: Bytes) -> Option<WebpPlayer> {
        let mut decoder = webp_decoder(&bytes)?;
        let (width, height) = decoder.dimensions();
        if !decoder.is_animated()
            || !(2..=MAX_ANIMATION_FRAMES).contains(&decoder.num_frames())
            || !fits(width, height)
        {
            return None;
        }
        let mut buffer = vec![0u8; decoder.output_buffer_size()?];
        let hold = delay(decoder.read_frame(&mut buffer).ok()?);
        Some(WebpPlayer {
            // A count of n means n plays in all.
            plays_left: match decoder.loop_count() {
                image_webp::LoopCount::Forever => None,
                image_webp::LoopCount::Times(n) => Some(u32::from(n.get()) - 1),
            },
            alpha: decoder.has_alpha(),
            bytes,
            decoder,
            buffer,
            width,
            height,
            hold,
            drawn: 1,
        })
    }

    fn next(&mut self) -> Option<Step> {
        let after = self.hold;
        match self.decoder.read_frame(&mut self.buffer) {
            Ok(ms) => self.hold = delay(ms),
            Err(_) => {
                if self.drawn < 2 {
                    return None;
                }
                match self.plays_left {
                    Some(0) => return None,
                    Some(n) => self.plays_left = Some(n - 1),
                    None => {}
                }
                // A fresh decoder starts from an empty canvas, as a loop does.
                self.decoder = webp_decoder(&self.bytes)?;
                self.drawn = 0;
                self.hold = delay(self.decoder.read_frame(&mut self.buffer).ok()?);
            }
        }
        self.drawn += 1;
        if self.drawn > MAX_ANIMATION_FRAMES {
            return None;
        }
        let rgba = if self.alpha {
            self.buffer.clone()
        } else {
            self.buffer
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect()
        };
        Some(Step {
            rgba,
            width: self.width,
            height: self.height,
            after,
            index: self.drawn - 1,
            forever: self.plays_left.is_none(),
            scaled: false,
        })
    }
}

pub(crate) enum Job {
    /// Open a player for these bytes under `token` and decode its second frame. `size` is the
    /// box the image is shown in when that is below half its size (see [`fit`]).
    Start {
        token: u64,
        bytes: Vec<u8>,
        size: Option<(u32, u32)>,
    },
    /// Decode the frame after the one last sent.
    Next(u64, Option<(u32, u32)>),
    /// Forget the player.
    Drop(u64),
}

pub(crate) enum Reply {
    Frame(u64, Step),
    /// The file does not animate, its last loop ended, or it could not be read further. The
    /// player is gone.
    End(u64),
}

/// Called on the worker thread after each reply; it must only post a message.
pub(crate) type Waker = Arc<dyn Fn() + Send + Sync>;

/// The thread that owns every player. It ends when the `Worker` is dropped.
pub(crate) struct Worker {
    pub jobs: Sender<Job>,
    pub replies: Receiver<Reply>,
}

impl Worker {
    pub(crate) fn spawn(waker: Option<Waker>) -> Option<Worker> {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, replies) = channel::<Reply>();
        std::thread::Builder::new()
            .name("mdview-anim".into())
            .spawn(move || run(inbox, outbox, waker))
            .ok()?;
        Some(Worker { jobs, replies })
    }
}

fn run(inbox: Receiver<Job>, outbox: Sender<Reply>, waker: Option<Waker>) {
    let mut players: HashMap<u64, Player> = HashMap::new();
    // One frame per request, so the worker is never more than one frame ahead of what is shown
    // and does nothing at all for an animation nobody is looking at.
    for job in inbox {
        // A decoder that panics on hostile input ends that animation, nothing else.
        let reply = match job {
            Job::Drop(token) => {
                players.remove(&token);
                continue;
            }
            Job::Start { token, bytes, size } => {
                let opened = catch_unwind(|| {
                    let mut player = Player::open(bytes)?;
                    let step = fit(player.next()?, size);
                    Some((player, step))
                });
                match opened.ok().flatten() {
                    Some((player, step)) => {
                        players.insert(token, player);
                        Reply::Frame(token, step)
                    }
                    None => Reply::End(token),
                }
            }
            Job::Next(token, size) => {
                let Some(player) = players.get_mut(&token) else {
                    continue;
                };
                match catch_unwind(AssertUnwindSafe(|| Some(fit(player.next()?, size)))) {
                    Ok(Some(step)) => Reply::Frame(token, step),
                    _ => {
                        players.remove(&token);
                        Reply::End(token)
                    }
                }
            }
        };
        if outbox.send(reply).is_err() {
            break;
        }
        if let Some(wake) = &waker {
            wake();
        }
    }
}

/// GIF files for tests: well-formed ones through the crate's encoder, hostile ones byte by byte.
#[cfg(test)]
pub(crate) mod testgif {
    use std::borrow::Cow;

    /// Black, red, green, blue.
    pub(crate) const PALETTE: [u8; 12] = [0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
    pub(crate) const RED: [u8; 4] = [255, 0, 0, 255];
    pub(crate) const GREEN: [u8; 4] = [0, 255, 0, 255];
    pub(crate) const BLUE: [u8; 4] = [0, 0, 255, 255];

    pub(crate) struct TestFrame {
        pub rect: (u16, u16, u16, u16),
        pub indices: Vec<u8>,
        /// Hundredths of a second.
        pub delay: u16,
        pub dispose: gif::DisposalMethod,
        pub transparent: Option<u8>,
    }

    /// A frame of one colour.
    pub(crate) fn solid(rect: (u16, u16, u16, u16), index: u8, delay: u16) -> TestFrame {
        TestFrame {
            rect,
            indices: vec![index; usize::from(rect.2) * usize::from(rect.3)],
            delay,
            dispose: gif::DisposalMethod::Keep,
            transparent: None,
        }
    }

    pub(crate) fn encode(
        width: u16,
        height: u16,
        repeat: Option<gif::Repeat>,
        frames: &[TestFrame],
    ) -> Vec<u8> {
        let mut encoder = gif::Encoder::new(Vec::new(), width, height, &PALETTE).unwrap();
        if let Some(repeat) = repeat {
            encoder.set_repeat(repeat).unwrap();
        }
        for frame in frames {
            encoder
                .write_frame(&gif::Frame {
                    left: frame.rect.0,
                    top: frame.rect.1,
                    width: frame.rect.2,
                    height: frame.rect.3,
                    delay: frame.delay,
                    dispose: frame.dispose,
                    transparent: frame.transparent,
                    buffer: Cow::Borrowed(&frame.indices),
                    ..gif::Frame::default()
                })
                .unwrap();
        }
        encoder.into_inner().unwrap()
    }

    /// A GIF written by hand: a two-colour global palette (black, white) and frames given as
    /// `(left, top, width, height, delay)`, each carrying the pixel data of a single white pixel
    /// whatever size it claims.
    pub(crate) fn raw(screen: (u16, u16), frames: &[(u16, u16, u16, u16, u16)]) -> Vec<u8> {
        let mut out = b"GIF89a".to_vec();
        out.extend(screen.0.to_le_bytes());
        out.extend(screen.1.to_le_bytes());
        out.extend([0x80, 0, 0]);
        out.extend([0, 0, 0, 255, 255, 255]);
        for &(left, top, width, height, delay) in frames {
            out.extend([0x21, 0xF9, 4, 0]);
            out.extend(delay.to_le_bytes());
            out.extend([0, 0]);
            out.push(0x2C);
            for value in [left, top, width, height] {
                out.extend(value.to_le_bytes());
            }
            out.push(0);
            // Minimum code size 2; codes clear, 1, end in three bits each.
            out.extend([2, 2, 0x4C, 0x01, 0]);
        }
        out.push(0x3B);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testgif::*;
    use super::*;
    use std::path::Path;

    fn asset(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/assets");
        std::fs::read(path.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn pixel(step: &Step, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * step.width + x) * 4) as usize;
        step.rgba[at..at + 4].try_into().unwrap()
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Four frames on a 4 x 4 canvas that use every disposal method, a transparent index and a
    /// zero delay.
    fn four_frames(repeat: Option<gif::Repeat>) -> Vec<u8> {
        let mut second = solid((1, 1, 2, 2), 2, 0);
        second.dispose = gif::DisposalMethod::Background;
        let third = TestFrame {
            rect: (0, 0, 2, 2),
            indices: vec![3, 0, 0, 3],
            delay: 20,
            dispose: gif::DisposalMethod::Previous,
            transparent: Some(0),
        };
        encode(
            4,
            4,
            repeat,
            &[
                solid((0, 0, 4, 4), 1, 5),
                second,
                third,
                solid((3, 3, 1, 1), 2, 3),
            ],
        )
    }

    #[test]
    fn gif_delays_disposal_transparency_and_loop_count() {
        // One repeat: two plays of four frames. The first frame of the first play is the
        // caller's, so seven steps follow.
        let mut player = Player::open(four_frames(Some(gif::Repeat::Finite(1)))).expect("animated");
        for play in 0..2 {
            if play == 1 {
                // The loop starts from an empty canvas; the last frame's 30 ms come first.
                let first = player.next().expect("first frame again");
                assert_eq!(first.after, ms(30));
                assert!(first.rgba.as_chunks::<4>().0.iter().all(|p| *p == RED));
            }
            let second = player.next().expect("second frame");
            assert_eq!((second.width, second.height), (4, 4));
            assert_eq!(second.after, ms(50));
            assert_eq!(pixel(&second, 0, 0), RED);
            assert_eq!(pixel(&second, 1, 1), GREEN);
            assert_eq!(pixel(&second, 2, 2), GREEN);
            assert_eq!(pixel(&second, 3, 3), RED);

            // A zero delay plays as 100 ms. The second frame's square was disposed to
            // transparent; the third draws two pixels and leaves its transparent two alone.
            let third = player.next().expect("third frame");
            assert_eq!(third.after, ms(100));
            assert_eq!(pixel(&third, 0, 0), BLUE);
            assert_eq!(pixel(&third, 1, 0), RED);
            assert_eq!(pixel(&third, 0, 1), RED);
            assert_eq!(pixel(&third, 1, 1), BLUE);
            assert_eq!(pixel(&third, 2, 2)[3], 0);
            assert_eq!(pixel(&third, 2, 1)[3], 0);

            // The third frame was disposed to what was there before it.
            let fourth = player.next().expect("fourth frame");
            assert_eq!(fourth.after, ms(200));
            assert_eq!(pixel(&fourth, 0, 0), RED);
            assert_eq!(pixel(&fourth, 1, 1)[3], 0);
            assert_eq!(pixel(&fourth, 3, 3), GREEN);
            assert_eq!(pixel(&fourth, 3, 0), RED);
        }
        assert!(player.next().is_none(), "two plays and no more");
        assert!(player.next().is_none());
    }

    #[test]
    fn gif_without_a_loop_block_plays_once_and_zero_means_forever() {
        let mut once = Player::open(four_frames(None)).expect("animated");
        assert_eq!(std::iter::from_fn(|| once.next()).count(), 3);
        let mut forever = Player::open(four_frames(Some(gif::Repeat::Infinite))).expect("animated");
        assert!((0..100).all(|_| forever.next().is_some()));
    }

    #[test]
    fn stills_have_no_player() {
        assert!(Player::open(asset("offset.gif")).is_none());
        assert!(Player::open(asset("lossy.webp")).is_none());
        assert!(Player::open(asset("alpha.webp")).is_none());
        assert!(Player::open(encode(4, 4, None, &[solid((0, 0, 4, 4), 1, 5)])).is_none());
        assert!(Player::open(Vec::new()).is_none());
        assert!(candidate(&asset("offset.gif")));
        assert!(candidate(&asset("anim.webp")));
        assert!(!candidate(&asset("lossy.webp")));
        assert!(!candidate(&asset("alpha.webp")));
        assert!(!candidate(&asset("badge.png")));
    }

    #[test]
    fn fixture_gif_and_webp_animate() {
        for name in ["anim.gif", "anim.webp"] {
            let bytes = asset(name);
            let first = crate::image::decode(&bytes).expect("first frame");
            let mut player = Player::open(bytes).unwrap_or_else(|| panic!("{name} animates"));
            let second = player
                .next()
                .unwrap_or_else(|| panic!("{name} second frame"));
            assert_eq!((second.width, second.height), (first.width, first.height));
            assert_eq!(second.rgba.len(), first.rgba.len());
            assert_ne!(second.rgba, first.rgba, "{name}: the second frame differs");
            assert!(second.after >= ms(20), "{name}: {:?}", second.after);
            // Both fixtures loop for ever, and come back to their first frame.
            let mut back = false;
            for _ in 0..50 {
                let step = player.next().unwrap_or_else(|| panic!("{name} loops"));
                back |= step.rgba == first.rgba;
            }
            assert!(back, "{name}: a loop shows the first frame again");
        }
    }

    #[test]
    fn hostile_gifs_are_bounded() {
        // More frames than the limit: no player, found without decoding a pixel.
        let many: Vec<_> = (0..=MAX_ANIMATION_FRAMES)
            .map(|_| (0, 0, 1, 1, 0))
            .collect();
        assert!(Player::open(raw((4, 4), &many)).is_none());

        // Zero delays everywhere: every frame still waits 100 ms.
        let fast: Vec<_> = (0..300).map(|_| (0, 0, 1, 1, 0)).collect();
        let mut player = Player::open(raw((4, 4), &fast)).expect("animated");
        let mut steps = 0;
        while let Some(step) = player.next() {
            assert_eq!(step.after, ms(100));
            steps += 1;
        }
        assert_eq!(steps, 299, "no loop block: one play");

        // A frame wholly outside the canvas draws nothing and is still a frame.
        let outside = raw(
            (4, 4),
            &[(0, 0, 1, 1, 7), (60000, 60000, 1, 1, 7), (9, 0, 1, 1, 7)],
        );
        let mut player = Player::open(outside).expect("animated");
        for _ in 0..2 {
            let step = player.next().expect("frame");
            assert_eq!(step.rgba.len(), 4 * 4 * 4);
            assert_eq!(pixel(&step, 0, 0), [255, 255, 255, 255]);
            assert_eq!(step.rgba.iter().filter(|&&b| b != 0).count(), 4);
        }

        // A second frame claiming 65535 x 65535 is refused before anything is allocated for it;
        // one frame is not an animation.
        let huge = raw((4, 4), &[(0, 0, 1, 1, 7), (0, 0, 65535, 65535, 7)]);
        let mut player = Player::open(huge).expect("two frame headers");
        assert!(player.next().is_none());

        // A canvas over the animation limit never gets a player.
        let wide = raw((5000, 5000), &[(0, 0, 1, 1, 7), (0, 0, 1, 1, 7)]);
        assert!(Player::open(wide).is_none());

        // A frame that says it is larger than its data: the frame is not drawn.
        let short = raw((40, 40), &[(0, 0, 1, 1, 7), (0, 0, 40, 40, 7)]);
        if let Some(mut player) = Player::open(short) {
            assert!(player.next().is_none());
        }
    }

    #[test]
    fn broken_files_neither_panic_nor_spin() {
        let whole = four_frames(Some(gif::Repeat::Infinite));
        let play = |bytes: Vec<u8>| {
            if let Some(mut player) = Player::open(bytes) {
                for _ in 0..40 {
                    if player.next().is_none() {
                        break;
                    }
                }
            }
        };
        for cut in 0..whole.len() {
            play(whole[..cut].to_vec());
        }
        for at in 0..whole.len() {
            for value in [0x00, 0xFF, 0x2C, 0x21] {
                let mut bytes = whole.clone();
                bytes[at] = value;
                play(bytes);
            }
        }
        let webp = asset("anim.webp");
        for cut in 0..webp.len() {
            play(webp[..cut].to_vec());
        }
        for at in 12..webp.len() {
            let mut bytes = webp.clone();
            bytes[at] ^= 0xFF;
            play(bytes);
        }
    }

    #[test]
    fn worker_decodes_one_frame_per_request() {
        let worker = Worker::spawn(None).expect("thread");
        let wait = Duration::from_secs(10);
        worker
            .jobs
            .send(Job::Start {
                token: 7,
                bytes: four_frames(Some(gif::Repeat::Infinite)),
                size: None,
            })
            .unwrap();
        let Ok(Reply::Frame(7, second)) = worker.replies.recv_timeout(wait) else {
            panic!("the second frame comes with the start");
        };
        assert_eq!(pixel(&second, 1, 1), GREEN);
        // Nothing more until asked.
        assert!(worker.replies.recv_timeout(ms(150)).is_err());
        worker.jobs.send(Job::Next(7, None)).unwrap();
        let Ok(Reply::Frame(7, third)) = worker.replies.recv_timeout(wait) else {
            panic!("the third frame");
        };
        assert_eq!(pixel(&third, 0, 0), BLUE);
        // A still image, and garbage, end at once.
        for (token, bytes) in [(8, asset("offset.gif")), (9, b"GIF89a\xFF".to_vec())] {
            let size = None;
            worker.jobs.send(Job::Start { token, bytes, size }).unwrap();
            assert!(matches!(
                worker.replies.recv_timeout(wait),
                Ok(Reply::End(t)) if t == token
            ));
        }
        // A dropped player answers nothing.
        worker.jobs.send(Job::Drop(7)).unwrap();
        worker.jobs.send(Job::Next(7, None)).unwrap();
        assert!(worker.replies.recv_timeout(ms(150)).is_err());
    }
}
