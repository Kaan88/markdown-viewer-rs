//! Minimal binary for size comparison: parse argv[1] and print the node count.
fn main() {
    let s = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let arena = comrak::Arena::new();
    let mut o = comrak::Options::default();
    o.extension.table = true;
    println!("{}", comrak::parse_document(&arena, &s, &o).descendants().count());
}
