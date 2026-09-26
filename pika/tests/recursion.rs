use pika::{parse_pairs, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RuleLR {
    EOI = 0,
    E = 1,
    T = 2,
}

fn to_rule_lr(idx: usize) -> RuleLR {
    match idx {
        0 => RuleLR::EOI,
        1 => RuleLR::E,
        2 => RuleLR::T,
        _ => unreachable!(),
    }
}

#[test]
fn test_left_recursion() {
    let grammar_src = r#"
E = { (E ~ ("+" | "-") ~ T) | T }
T = { ASCII_ALPHA_LOWER | ASCII_DIGIT }
"#;
    let grammar = Grammar::compile(grammar_src).expect("grammar should compile");
    let input = "a+b-c";
    let mut pairs = parse_pairs(&grammar, 1, input, to_rule_lr).expect("should parse");

    assert_eq!(pairs.len(), 1);
    let root = pairs.next().unwrap();
    assert_eq!(root.as_rule(), RuleLR::E);
    assert_eq!(root.as_span().start(), 0);
    assert_eq!(root.as_span().end(), 5);

    // root E (0..5) has first child E (0..3)
    let mut root_inner = root.into_inner();
    let child_e1 = root_inner.next().unwrap();
    assert_eq!(child_e1.as_rule(), RuleLR::E);
    assert_eq!(child_e1.as_span().start(), 0);
    assert_eq!(child_e1.as_span().end(), 3);

    // child E (0..3) has first child E (0..1)
    let mut child_e1_inner = child_e1.into_inner();
    let child_e0 = child_e1_inner.next().unwrap();
    assert_eq!(child_e0.as_rule(), RuleLR::E);
    assert_eq!(child_e0.as_span().start(), 0);
    assert_eq!(child_e0.as_span().end(), 1);
    assert_eq!(child_e0.as_str(), "a");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RulePrec {
    EOI = 0,
    E = 1,
}

fn to_rule_prec(idx: usize) -> RulePrec {
    match idx {
        0 => RulePrec::EOI,
        1 => RulePrec::E,
        _ => unreachable!(),
    }
}

#[test]
fn test_precedence_and_associativity() {
    let grammar_src = r#"
E[4] = { "(" ~ E ~ ")" }
E[3] = { ASCII_DIGIT+ | ASCII_ALPHA_LOWER+ }
E[2] = { "-" ~ E }
E[1,L] = { E ~ ("*" | "/") ~ E }
E[0,L] = { E ~ ("+" | "-") ~ E }
"#;
    let grammar = Grammar::compile(grammar_src).expect("grammar should compile");
    let input = "b*b-4*a*c";
    let mut pairs = parse_pairs(&grammar, 1, input, to_rule_prec).expect("should parse");

    assert_eq!(pairs.len(), 1);
    let root = pairs.next().unwrap();
    assert_eq!(root.as_rule(), RulePrec::E);
    assert_eq!(root.as_span().start(), 0);
    assert_eq!(root.as_span().end(), 9);

    // Root is minus: b*b - 4*a*c
    // Children of root E (0..9): E (0..3: "b*b"), E (4..9: "4*a*c")
    let root_children: Vec<_> = root.clone().into_inner().collect();
    assert_eq!(root_children.len(), 2);

    let left = &root_children[0];
    assert_eq!(left.as_rule(), RulePrec::E);
    assert_eq!(left.as_span().start(), 0);
    assert_eq!(left.as_span().end(), 3);
    assert_eq!(left.as_str(), "b*b");

    let right = &root_children[1];
    assert_eq!(right.as_rule(), RulePrec::E);
    assert_eq!(right.as_span().start(), 4);
    assert_eq!(right.as_span().end(), 9);
    assert_eq!(right.as_str(), "4*a*c");

    // Right child E(4..9) has first child E(4..7: "4*a")
    let right_children: Vec<_> = right.clone().into_inner().collect();
    assert_eq!(right_children.len(), 2);
    let right_left = &right_children[0];
    assert_eq!(right_left.as_rule(), RulePrec::E);
    assert_eq!(right_left.as_span().start(), 4);
    assert_eq!(right_left.as_span().end(), 7);
    assert_eq!(right_left.as_str(), "4*a");

    // Assert precedence-collapse rule: no E Pair has a single E child with an identical span
    for pair in pairs.flatten() {
        let span = pair.as_span();
        let inner: Vec<_> = pair.clone().into_inner().collect();
        if inner.len() == 1 && inner[0].as_rule() == pair.as_rule() {
            assert_ne!(
                (inner[0].as_span().start(), inner[0].as_span().end()),
                (span.start(), span.end()),
                "Precedence collapse failed on {:?}",
                pair
            );
        }
    }
}
