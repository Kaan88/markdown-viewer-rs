//! Everything the harness renders: the 8 showcase blocks, the 8 core_tests sources, and two
//! harder real-world diagrams per family with longer labels.

pub const FAMILIES: [&str; 8] = ["flowchart", "sequence", "class", "state", "er", "gantt", "pie", "mindmap"];

/// Sources from the plan's core_tests, in FAMILIES order.
pub const CORE: [&str; 8] = [
    "flowchart LR\n A[Start] --> B[Done]",
    "sequenceDiagram\n Alice->>Bob: Hello",
    "classDiagram\n Animal <|-- Dog",
    "stateDiagram-v2\n [*] --> Ready\n Ready --> [*]",
    "erDiagram\n CUSTOMER ||--o{ ORDER : places",
    "gantt\n dateFormat YYYY-MM-DD\n section Work\n Task :a, 2026-01-01, 3d",
    "pie\n \"A\" : 40\n \"B\" : 60",
    "mindmap\n root((Root))\n  Child",
];

/// Words every core_tests render must paint, in FAMILIES order.
pub const CORE_EXPECT: [&[&str]; 8] = [
    &["Start", "Done"],
    &["Alice", "Bob", "Hello"],
    &["Animal", "Dog"],
    &["Ready"],
    &["CUSTOMER", "ORDER", "places"],
    &["Work", "Task"],
    &["A", "B"],
    &["Root", "Child"],
];

/// Words every showcase render must paint, in FAMILIES order.
pub const SHOWCASE_EXPECT: [&[&str]; 8] = [
    &["Open file", "Parse Markdown", "Native viewer"],
    &["Reader", "App", "Open README", "Render document"],
    &["Document", "Markdown", "path"],
    &["Loading", "Ready"],
    &["FOLDER", "DOCUMENT", "contains"],
    &["Reading plan", "Documentation", "Read README", "Explore docs"],
    &["File types", "Markdown", "Images"],
    &["Reading", "Files", "README", "Outline", "Search"],
];

const SHOWCASE: &str = include_str!("../../../fixtures/showcase.md");

/// The eight ```mermaid blocks, in fixture order (matches FAMILIES).
pub fn showcase() -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for line in SHOWCASE.lines() {
        match (&mut cur, line.trim_end()) {
            (None, "```mermaid") => cur = Some(String::new()),
            (Some(_), "```") => out.push(cur.take().unwrap()),
            (Some(b), l) => {
                b.push_str(l);
                b.push('\n');
            }
            _ => {}
        }
    }
    assert_eq!(out.len(), 8, "expected 8 mermaid blocks in showcase.md");
    out
}

/// (name, source) for the harder set. Each file starts with `%% expect: a|b|c`.
pub const HARD: [(&str, &str); 16] = [
    ("flowchart-a", include_str!("../corpus/flowchart-a.mmd")),
    ("flowchart-b", include_str!("../corpus/flowchart-b.mmd")),
    ("sequence-a", include_str!("../corpus/sequence-a.mmd")),
    ("sequence-b", include_str!("../corpus/sequence-b.mmd")),
    ("class-a", include_str!("../corpus/class-a.mmd")),
    ("class-b", include_str!("../corpus/class-b.mmd")),
    ("state-a", include_str!("../corpus/state-a.mmd")),
    ("state-b", include_str!("../corpus/state-b.mmd")),
    ("er-a", include_str!("../corpus/er-a.mmd")),
    ("er-b", include_str!("../corpus/er-b.mmd")),
    ("gantt-a", include_str!("../corpus/gantt-a.mmd")),
    ("gantt-b", include_str!("../corpus/gantt-b.mmd")),
    ("pie-a", include_str!("../corpus/pie-a.mmd")),
    ("pie-b", include_str!("../corpus/pie-b.mmd")),
    ("mindmap-a", include_str!("../corpus/mindmap-a.mmd")),
    ("mindmap-b", include_str!("../corpus/mindmap-b.mmd")),
];

/// Stress probes beyond the two-per-family set. Not part of the per-family verdict.
pub const PROBES: [(&str, &str); 2] = [
    ("pie-long-title", include_str!("../corpus/probe-pie-long-title.mmd")),
    ("scripts", include_str!("../corpus/probe-scripts.mmd")),
];

pub fn expected_of(src: &str) -> Vec<&str> {
    src.lines()
        .next()
        .and_then(|l| l.strip_prefix("%% expect: "))
        .map(|l| l.split('|').collect())
        .unwrap_or_default()
}

/// Every diagram with a file-safe name.
pub fn all() -> Vec<(String, String)> {
    let mut v = Vec::new();
    for (i, s) in showcase().into_iter().enumerate() {
        v.push((format!("showcase-{}", FAMILIES[i]), s));
    }
    for (i, s) in CORE.iter().enumerate() {
        v.push((format!("core-{}", FAMILIES[i]), s.to_string()));
    }
    for (n, s) in HARD {
        v.push((format!("hard-{n}"), s.to_string()));
    }
    for (n, s) in PROBES {
        v.push((format!("probe-{n}"), s.to_string()));
    }
    // Size probes. A README's largest realistic diagrams are well under these.
    let mut f = String::from("flowchart TD\n");
    for i in 0..150 {
        f.push_str(&format!(" n{i}[Step number {i} of the pipeline] --> n{}[Step number {} of the pipeline]\n", i + 1, i + 1));
        if i % 5 == 0 && i > 0 {
            f.push_str(&format!(" n{i} --> n{}\n", i / 2));
        }
    }
    v.push(("probe-flow-150".into(), f));
    let mut q = String::from("sequenceDiagram\n");
    for i in 0..200 {
        q.push_str(&format!(" P{}->>P{}: Message number {i} between participants\n", i % 8, (i + 3) % 8));
    }
    v.push(("probe-seq-200".into(), q));
    v
}
