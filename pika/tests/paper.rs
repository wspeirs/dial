use pika::{parse_pairs, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Rule {
    EOI = 0,
    P = 1,
    V = 2,
}

fn to_rule(idx: usize) -> Rule {
    match idx {
        0 => Rule::EOI,
        1 => Rule::P,
        2 => Rule::V,
        _ => unreachable!(),
    }
}

#[test]
fn test_paper_fig_3() {
    let grammar_src = r#"
P = { V+ }
V = { 'a'..'z' | ("(" ~ P ~ ")") }
"#;
    let grammar = Grammar::compile(grammar_src).expect("grammar should compile");
    let input = "a(bc)d";
    let mut pairs = parse_pairs(&grammar, 1, input, to_rule).expect("should parse");

    assert_eq!(pairs.len(), 1);
    let root = pairs.next().unwrap();
    assert_eq!(root.as_rule(), Rule::P);
    assert_eq!(root.as_span().start(), 0);
    assert_eq!(root.as_span().end(), 6);

    let v_children: Vec<_> = root.into_inner().collect();
    assert_eq!(v_children.len(), 3);

    // v0: "a" (0..1)
    assert_eq!(v_children[0].as_rule(), Rule::V);
    assert_eq!(v_children[0].as_span().start(), 0);
    assert_eq!(v_children[0].as_span().end(), 1);
    assert_eq!(v_children[0].as_str(), "a");

    // v1: "(bc)" (1..5)
    assert_eq!(v_children[1].as_rule(), Rule::V);
    assert_eq!(v_children[1].as_span().start(), 1);
    assert_eq!(v_children[1].as_span().end(), 5);
    assert_eq!(v_children[1].as_str(), "(bc)");

    // v1 has child P spanning 2..4
    let mut inner_p = v_children[1].clone().into_inner();
    let p_mid = inner_p.next().unwrap();
    assert_eq!(p_mid.as_rule(), Rule::P);
    assert_eq!(p_mid.as_span().start(), 2);
    assert_eq!(p_mid.as_span().end(), 4);

    let p_mid_v: Vec<_> = p_mid.into_inner().collect();
    assert_eq!(p_mid_v.len(), 2);
    assert_eq!(p_mid_v[0].as_str(), "b");
    assert_eq!(p_mid_v[1].as_str(), "c");

    // v2: "d" (5..6)
    assert_eq!(v_children[2].as_rule(), Rule::V);
    assert_eq!(v_children[2].as_span().start(), 5);
    assert_eq!(v_children[2].as_span().end(), 6);
    assert_eq!(v_children[2].as_str(), "d");
}
