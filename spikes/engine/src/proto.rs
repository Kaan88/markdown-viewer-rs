//! Framed wire format: [u32 len][u8 kind][u32 id][payload], little endian. len = 5 + payload.
use std::io::{self, Read, Write};

pub const REQ_MATH: u8 = 1; // u8 display, f32 em_px, utf8 tex
pub const REQ_HIGHLIGHT: u8 = 2; // u16 lang_len, lang, code
pub const REQ_SLEEP: u8 = 101; // u32 ms
pub const REQ_BOMB_TOUCH: u8 = 102;
pub const REQ_BOMB_HUGE: u8 = 103;
pub const REQ_PANIC: u8 = 104;
pub const REQ_STACK: u8 = 105;

pub const REP_PIXELS: u8 = 1; // metrics, u32 w, u32 h, u32 baseline, BGRA premultiplied
pub const REP_SPANS: u8 = 2; // metrics, u32 n, n * (u32 byte_len, u32 rgba)
pub const REP_ERROR: u8 = 3; // metrics, utf8 message

pub const MAX_TEX_BYTES: usize = 4096;
pub const MAX_REQUEST_FRAME: usize = 1 << 20;
pub const MAX_REPLY_FRAME: usize = 128 << 20;

pub const N_METRICS: usize = 10;

#[derive(Default, Clone, Copy, Debug)]
pub struct Metrics {
    pub parse_us: u32,
    pub layout_us: u32,
    pub svg_us: u32,
    pub raster_us: u32,
    pub load_us: u32,
    pub total_us: u32,
    pub uptime_us: u32,
    pub vm_hwm_kb: u32,
    pub vm_rss_kb: u32,
    pub vm_peak_kb: u32,
}

impl Metrics {
    pub fn encode(&self) -> Vec<u8> {
        let a = [
            self.parse_us, self.layout_us, self.svg_us, self.raster_us, self.load_us,
            self.total_us, self.uptime_us, self.vm_hwm_kb, self.vm_rss_kb, self.vm_peak_kb,
        ];
        a.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
    pub fn decode(b: &[u8]) -> Metrics {
        let g = |i: usize| u32_at(b, i * 4);
        Metrics {
            parse_us: g(0), layout_us: g(1), svg_us: g(2), raster_us: g(3), load_us: g(4),
            total_us: g(5), uptime_us: g(6), vm_hwm_kb: g(7), vm_rss_kb: g(8), vm_peak_kb: g(9),
        }
    }
}

pub fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

pub fn encode_frame(kind: u8, id: u32, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(9 + payload.len());
    f.extend_from_slice(&((payload.len() as u32 + 5).to_le_bytes()));
    f.push(kind);
    f.extend_from_slice(&id.to_le_bytes());
    f.extend_from_slice(payload);
    f
}

pub struct Frame {
    pub kind: u8,
    pub id: u32,
    pub payload: Vec<u8>,
    /// The frame exceeded the cap: payload was drained and dropped, never buffered.
    pub oversized: Option<usize>,
}

pub fn read_frame(r: &mut impl Read, cap: usize) -> io::Result<Frame> {
    let mut h = [0u8; 9];
    r.read_exact(&mut h)?;
    let len = u32_at(&h, 0) as usize;
    if len < 5 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "short frame"));
    }
    let (kind, id, body) = (h[4], u32_at(&h, 5), len - 5);
    if body > cap {
        io::copy(&mut r.take(body as u64), &mut io::sink())?;
        return Ok(Frame { kind, id, payload: Vec::new(), oversized: Some(body) });
    }
    let mut payload = vec![0u8; body];
    r.read_exact(&mut payload)?;
    Ok(Frame { kind, id, payload, oversized: None })
}

pub fn write_all_flush(w: &mut impl Write, b: &[u8]) -> io::Result<()> {
    w.write_all(b)?;
    w.flush()
}
