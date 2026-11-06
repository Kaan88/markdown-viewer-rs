//! Minimal binary for size comparison: parse argv[1] and print the event count.
fn main() {
    let s = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    println!("{}", pulldown_cmark::Parser::new_ext(&s, pulldown_cmark::Options::all()).count());
}
