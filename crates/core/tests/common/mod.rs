//! Shared test support: a writer-neutral HTML rendering of the document model, the comparison
//! pipeline for the spec suites, and a seeded random generator.
#![allow(dead_code)]

pub mod normalize;
pub mod visible;
pub mod writer;

/// xorshift64*, enough for reproducible test data.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n
    }
    pub fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

/// The comparison form used on both sides of every spec example.
pub fn neutral(html: &str) -> String {
    normalize::semantic(&normalize::normalize(&visible::visible(html)))
}

/// A writable directory for a test's files. `%TEMP%` can be blocked by a sandbox even when the
/// workspace is writable, which is the case on the Windows host this repo is developed on, so this
/// falls back to a workspace-local `target/tmp` rather than failing the test.
pub fn temp_dir(name: &str) -> std::path::PathBuf {
    let in_root = |root: std::path::PathBuf| {
        let dir = root.join(format!("mdview-core-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).ok().map(|()| dir)
    };
    in_root(std::env::temp_dir())
        .or_else(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
            in_root(root)
        })
        .expect("a writable directory for test files")
}
