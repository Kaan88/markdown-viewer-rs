use resvg::{tiny_skia::*, usvg};
fn main(){ let a:Vec<String>=std::env::args().collect();
 let mut o=usvg::Options::default(); let db=o.fontdb_mut();
 db.load_font_data(std::fs::read("/usr/share/fonts/liberation/LiberationSans-Regular.ttf").unwrap());
 db.load_font_data(std::fs::read("/usr/share/fonts/liberation/LiberationSans-Bold.ttf").unwrap());
 db.set_sans_serif_family("Liberation Sans"); o.font_family="Liberation Sans".into();
 let svg=std::fs::read_to_string(&a[1]).unwrap().replace("overflow:hidden","overflow:visible"); let svg=svg.replace("stroke-width:-1","stroke-width:2").replace("stroke-width:-4","stroke-width:2");
 let t=usvg::Tree::from_str(&svg,&o).unwrap(); let s=t.size();
 let mut p=Pixmap::new((s.width()*2.).ceil() as u32,(s.height()*2.).ceil() as u32).unwrap(); p.fill(Color::WHITE);
 resvg::render(&t,Transform::from_scale(2.,2.),&mut p.as_mut()); p.save_png(&a[2]).unwrap(); }
