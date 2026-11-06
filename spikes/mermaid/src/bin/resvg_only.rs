//! Size baseline: resvg with text and nothing else. merman's added size is the difference.
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut o = usvg::Options::default();
    for f in &a[2..] {
        o.fontdb_mut().load_font_data(std::fs::read(f).unwrap());
    }
    let t = usvg::Tree::from_str(&std::fs::read_to_string(&a[1]).unwrap(), &o).unwrap();
    let s = t.size();
    let mut p = Pixmap::new(s.width().ceil() as u32, s.height().ceil() as u32).unwrap();
    resvg::render(&t, Transform::identity(), &mut p.as_mut());
    println!("{}", p.encode_png().unwrap().len());
}
