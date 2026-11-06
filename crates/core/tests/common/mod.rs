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
