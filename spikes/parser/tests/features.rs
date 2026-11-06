//! Confirms the events the viewer depends on, for both parsers.
use comrak::nodes::{AlertType, NodeValue};
use parser_spike::*;
use pulldown_cmark::{BlockQuoteKind as K, CodeBlockKind, Event, Parser, Tag, TagEnd};

fn pd(md: &str) -> Vec<Event<'_>> {
    Parser::new_ext(md, pd_viewer()).collect()
}
fn cm<R>(md: &str, f: impl Fn(&NodeValue) -> Option<R>) -> Vec<R> {
    let arena = comrak::Arena::new();
    let root = comrak::parse_document(&arena, md, &cm_viewer());
    root.descendants().filter_map(|n| f(&n.data().value)).collect()
}

#[test]
fn alerts_all_five() {
    for (s, k) in [
        ("NOTE", K::Note),
        ("TIP", K::Tip),
        ("IMPORTANT", K::Important),
        ("WARNING", K::Warning),
        ("CAUTION", K::Caution),
    ] {
        let md = format!("> [!{s}]\n> body text\n");
        let ev = pd(&md);
        assert!(ev.iter().any(|e| matches!(e, Event::Start(Tag::BlockQuote(Some(x))) if *x == k)), "{s}: {ev:?}");
        assert!(!ev.iter().any(|e| matches!(e, Event::Text(t) if t.contains("[!"))), "marker leaked: {ev:?}");
        assert!(ev.iter().any(|e| matches!(e, Event::Text(t) if t.as_ref() == "body text")));
    }
    assert_eq!(cm("> [!WARNING]\n> x\n", |v| matches!(v, NodeValue::Alert(a) if a.alert_type == AlertType::Warning).then_some(())).len(), 1);
    // Not an alert: unknown kind stays a plain quote.
    assert!(pd("> [!FOO]\n> x\n").iter().any(|e| matches!(e, Event::Start(Tag::BlockQuote(None)))));
}

#[test]
fn footnotes() {
    let md = "A ref[^one].\n\n[^one]: Body.\n";
    let ev = pd(md);
    assert!(ev.iter().any(|e| matches!(e, Event::FootnoteReference(n) if n.as_ref() == "one")));
    assert!(ev.iter().any(|e| matches!(e, Event::Start(Tag::FootnoteDefinition(n)) if n.as_ref() == "one")));
    assert!(ev.iter().any(|e| matches!(e, Event::End(TagEnd::FootnoteDefinition))));
    assert_eq!(cm(md, |v| matches!(v, NodeValue::FootnoteDefinition(_)).then_some(())).len(), 1);
    assert_eq!(cm(md, |v| matches!(v, NodeValue::FootnoteReference(_)).then_some(())).len(), 1);
}

#[test]
fn task_lists() {
    let md = "- [ ] open\n- [x] done\n- [X] upper\n";
    let got: Vec<bool> = pd(md).iter().filter_map(|e| if let Event::TaskListMarker(b) = e { Some(*b) } else { None }).collect();
    assert_eq!(got, [false, true, true]);
    assert_eq!(cm(md, |v| if let NodeValue::TaskItem(t) = v { Some(t.symbol.is_some()) } else { None }), [false, true, true]);
}

#[test]
fn strikethrough() {
    for md in ["~~gone~~ text\n", "~gone~ text\n"] {
        assert!(pd(md).iter().any(|e| matches!(e, Event::Start(Tag::Strikethrough))), "{md}");
        assert_eq!(cm(md, |v| matches!(v, NodeValue::Strikethrough).then_some(())).len(), 1, "{md}");
    }
}

#[test]
fn math_inline_display_and_fence() {
    let md = "Inline $e^{i\\pi}+1=0$ end.\n\n$$\n\\sum_i x_i\n$$\n\n```math\nx^2\n```\n";
    let ev = pd(md);
    assert!(ev.iter().any(|e| matches!(e, Event::InlineMath(t) if t.as_ref() == "e^{i\\pi}+1=0")), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, Event::DisplayMath(t) if t.contains("\\sum_i x_i"))), "{ev:?}");
    // GitHub's ```math fence is an ordinary code block; the viewer must route it by info string.
    assert!(ev.iter().any(|e| matches!(e, Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(i))) if i.as_ref() == "math")));
    let n = cm(md, |v| if let NodeValue::Math(m) = v { Some(m.display_math) } else { None });
    assert_eq!(n, [false, true]);
    // Currency must not turn into math.
    assert!(!pd("It costs $5 and $10 total.\n").iter().any(|e| matches!(e, Event::InlineMath(_))));
}

#[test]
fn showcase_fixture_events() {
    let md = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/showcase.md")).unwrap();
    let ev: Vec<_> = autolink(Parser::new_ext(&md, pd_viewer())).collect();
    let n = |f: &dyn Fn(&Event) -> bool| ev.iter().filter(|e| f(e)).count();
    assert!(n(&|e| matches!(e, Event::Start(Tag::BlockQuote(Some(_))))) >= 1);
    assert!(n(&|e| matches!(e, Event::FootnoteReference(_))) >= 1);
    assert!(n(&|e| matches!(e, Event::Start(Tag::FootnoteDefinition(_)))) >= 1);
    assert!(n(&|e| matches!(e, Event::InlineMath(_))) >= 2);
    assert!(n(&|e| matches!(e, Event::DisplayMath(_))) >= 1);
    assert!(n(&|e| matches!(e, Event::TaskListMarker(_))) >= 1, "task markers");
    assert!(n(&|e| matches!(e, Event::Start(Tag::Strikethrough))) >= 1, "strike");
}

#[test]
fn header_only_table_has_no_rows_in_events() {
    let ev = pd("| a | b |\n| - | - |\n");
    assert!(ev.iter().any(|e| matches!(e, Event::Start(Tag::TableHead))));
    assert!(!ev.iter().any(|e| matches!(e, Event::Start(Tag::TableRow))));
}
