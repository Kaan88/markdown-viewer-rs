//! Plain Rust exe with no Windows crates, showing what Wine can run from this cross-toolchain.
fn main() {
    let dir = std::env::current_dir().unwrap();
    let path = dir.join("wine-smoke-out.txt");
    let sum: u64 = (0..1000u64).sum();
    std::fs::write(&path, format!("sum={sum}\n")).unwrap();
    println!(
        "wine-smoke ok: os={} arch={} sum={sum} read_back={:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::fs::read_to_string(&path).unwrap().trim()
    );
}
