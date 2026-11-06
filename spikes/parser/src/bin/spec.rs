//! Conformance: `spec <spec.json> <gfm spec.txt> <results dir>`.
use parser_spike::*;
use pulldown_cmark::Options as P;
use std::fmt::Write as _;
use std::fs;

struct Ex {
    n: usize,
    line: usize,
    section: String,
    md: String,
    html: String,
    tags: Vec<String>,
}

fn load_gfm(path: &str) -> Vec<Ex> {
    let fence = "`".repeat(32);
    let text = fs::read_to_string(path).unwrap();
    let (mut out, mut sec, mut state) = (Vec::new(), String::new(), 0);
    let (mut md, mut html, mut tags, mut start) = (String::new(), String::new(), Vec::new(), 0);
    for (i, l) in text.lines().enumerate() {
        if state == 0 {
            if let Some(rest) = l.strip_prefix(&format!("{fence} example")) {
                tags = rest.split_whitespace().map(str::to_string).collect();
                state = 1;
                start = i + 1;
                md.clear();
                html.clear();
            } else if l.starts_with('#') {
                sec = l.trim_start_matches('#').trim().to_string();
            }
        } else if l == fence {
            out.push(Ex {
                n: out.len() + 1,
                line: start,
                section: sec.clone(),
                md: md.replace('\u{2192}', "\t"),
                html: html.replace('\u{2192}', "\t"),
                tags: tags.clone(),
            });
            state = 0;
        } else if l == "." && state == 1 {
            state = 2;
        } else {
            let b = if state == 1 { &mut md } else { &mut html };
            b.push_str(l);
            b.push('\n');
        }
    }
    out
}

fn load_cm(path: &str) -> Vec<Ex> {
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| Ex {
            n: e["example"].as_u64().unwrap() as usize,
            line: e["start_line"].as_u64().unwrap() as usize,
            section: e["section"].as_str().unwrap().into(),
            md: e["markdown"].as_str().unwrap().into(),
            html: e["html"].as_str().unwrap().into(),
            tags: vec![],
        })
        .collect()
}

type Run<'a> = (&'a str, Box<dyn Fn(&Ex) -> String + 'a>);

fn has(e: &Ex, t: &str) -> bool {
    e.tags.iter().any(|x| x == t)
}
/// Official runner semantics: an example only gets the extensions it is tagged with.
/// The two `disabled` examples are task lists, so they get the task-list extension.
fn pd_tagged(e: &Ex) -> P {
    let mut o = P::empty();
    if has(e, "table") {
        o |= P::ENABLE_TABLES;
    }
    if has(e, "strikethrough") {
        o |= P::ENABLE_STRIKETHROUGH;
    }
    if has(e, "disabled") {
        o |= P::ENABLE_TASKLISTS;
    }
    o
}
fn cm_tagged(e: &Ex) -> comrak::Options<'static> {
    let mut o = cm_base();
    o.extension.table = has(e, "table");
    o.extension.strikethrough = has(e, "strikethrough");
    o.extension.autolink = has(e, "autolink");
    o.extension.tasklist = has(e, "disabled");
    o
}

fn run(exs: &[Ex], name: &str, dir: &str, f: &dyn Fn(&Ex) -> String, report: &mut String) -> Vec<usize> {
    let (mut exact, mut norm, mut sem, mut fails) = (0, 0, 0, String::new());
    let mut failed = Vec::new();
    for e in exs {
        let got = f(e);
        if got == e.html {
            exact += 1;
        }
        let (a, b) = (normalize(&got), normalize(&e.html));
        if a == b {
            norm += 1;
        }
        let (a, b) = (semantic(&a), semantic(&b));
        if a == b {
            sem += 1;
        } else {
            failed.push(e.n);
            let tags = e.tags.join(",");
            writeln!(
                fails,
                "=== ex {} line {} [{}] tags=[{}]\n--- md\n{}--- want\n{}\n--- got\n{}\n",
                e.n, e.line, e.section, tags, e.md, b, a
            )
            .unwrap();
        }
    }
    let total = exs.len();
    let pct = |p: usize| (total - p) as f64 * 100.0 / total as f64;
    let line = format!(
        "{name:<36} /{total} exact {exact:>3} | official-norm {norm:>3} miss {:>2} {:.2}% | semantic {sem:>3} miss {:>2} {:.2}%\n",
        total - norm,
        pct(norm),
        total - sem,
        pct(sem)
    );
    print!("{line}");
    report.push_str(&line);
    fs::write(format!("{dir}/fail_{name}.txt"), fails).unwrap();
    failed
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (cm, gfm, dir) = (load_cm(&a[1]), load_gfm(&a[2]), a[3].clone());
    fs::create_dir_all(&dir).unwrap();
    let mut r = String::new();
    println!("CommonMark examples {}, GFM examples {}", cm.len(), gfm.len());
    let cb = cm_base();

    println!("-- CommonMark 0.31.2 spec.json");
    run(&cm, "cm_pulldown", &dir, &|e| html_pulldown(&e.md, P::empty(), false), &mut r);
    run(&cm, "cm_comrak", &dir, &|e| html_comrak(&e.md, &cb), &mut r);

    let (gp, vp, gc, vc) = (pd_gfm(), pd_viewer(), cm_gfm(), cm_viewer());
    let runs: Vec<Run> = vec![
        ("gfm_tagged_pulldown_noautolink", Box::new(|e| html_pulldown(&e.md, pd_tagged(e), false))),
        ("gfm_tagged_pulldown_autolink", Box::new(|e| html_pulldown(&e.md, pd_tagged(e), has(e, "autolink")))),
        ("gfm_tagged_comrak", Box::new(|e| html_comrak(&e.md, &cm_tagged(e)))),
        ("gfm_alwayson_pulldown_noautolink", Box::new(|e| html_pulldown(&e.md, gp, false))),
        ("gfm_alwayson_pulldown_autolink", Box::new(|e| html_pulldown(&e.md, gp, true))),
        ("gfm_alwayson_comrak", Box::new(|e| html_comrak(&e.md, &gc))),
        ("gfm_viewer_pulldown_autolink", Box::new(|e| html_pulldown(&e.md, vp, true))),
        ("gfm_viewer_comrak", Box::new(|e| html_comrak(&e.md, &vc))),
    ];
    println!("-- cmark-gfm spec.txt (tagged = extensions only where the example is tagged; alwayson = all on)");
    let mut sets = Vec::new();
    for (name, f) in &runs {
        sets.push((*name, run(&gfm, name, &dir, f.as_ref(), &mut r)));
    }
    let get = |n: &str| sets.iter().find(|s| s.0 == n).unwrap().1.clone();
    for (label, p, c) in [
        ("tagged", "gfm_tagged_pulldown_autolink", "gfm_tagged_comrak"),
        ("alwayson", "gfm_alwayson_pulldown_autolink", "gfm_alwayson_comrak"),
    ] {
        let (p, c) = (get(p), get(c));
        let only_p: Vec<_> = p.iter().filter(|x| !c.contains(x)).collect();
        let only_c: Vec<_> = c.iter().filter(|x| !p.contains(x)).collect();
        let both: Vec<_> = p.iter().filter(|x| c.contains(x)).collect();
        let l = format!("{label}: pulldown-only misses {only_p:?}\n{label}: comrak-only misses {only_c:?}\n{label}: both miss {both:?}\n");
        print!("{l}");
        r.push_str(&l);
    }
    if let Some(doc) = a.get(4) {
        let doc = fs::read_to_string(doc).unwrap();
        // Multiset compare, since footnote placement legitimately differs. Block-closer split.
        let split = |h: String| -> Vec<String> {
            let n = semantic(&normalize(&h));
            let mut v = vec![];
            let mut cur = String::new();
            for tok in n.split_inclusive('>') {
                cur.push_str(tok);
                let closes = ["</p>", "</li>", "</tr>", "</blockquote>", "</pre>", "</h1>", "</h2>", "</h3>", "</h4>", "</ul>", "</ol>", "</table>", "</thead>", "</tbody>", "</div>", "</details>", "</summary>"];
                if closes.iter().any(|c| tok.ends_with(c)) {
                    v.push(std::mem::take(&mut cur));
                }
            }
            v.push(cur);
            v
        };
        let (mut x, mut y) = (split(html_pulldown(&doc, gp, true)), split(html_comrak(&doc, &gc)));
        x.sort();
        y.sort();
        let (mut i, mut j) = (0, 0);
        let (mut only_p, mut only_c) = (vec![], vec![]);
        while i < x.len() || j < y.len() {
            if j == y.len() || (i < x.len() && x[i] < y[j]) {
                only_p.push(x[i].clone());
                i += 1;
            } else if i == x.len() || y[j] < x[i] {
                only_c.push(y[j].clone());
                j += 1;
            } else {
                i += 1;
                j += 1;
            }
        }
        let l = format!(
            "1MiB doc, gfm options, multiset of chunks: pulldown {} comrak {} only-pulldown {} only-comrak {}\n",
            x.len(), y.len(), only_p.len(), only_c.len()
        );
        print!("{l}");
        r.push_str(&l);
        let mut d = String::new();
        for c in only_p.iter() {
            writeln!(d, "P: {c}").unwrap();
        }
        for c in only_c.iter() {
            writeln!(d, "C: {c}").unwrap();
        }
        fs::write(format!("{dir}/doc_diff.txt"), d).unwrap();
    }
    fs::write(format!("{dir}/spec_summary.txt"), r).unwrap();
}
