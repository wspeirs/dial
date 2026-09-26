use pika::{parse_pairs, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
enum Rule {
    EOI = 0,
    r1 = 1,
    inner = 2,
}
fn to_rule(idx: usize) -> Rule {
    match idx {
        0 => Rule::EOI,
        1 => Rule::r1,
        2 => Rule::inner,
        _ => unreachable!(),
    }
}

#[test]
fn test_positive_lookahead() {
    // &e should not consume input
    let g = Grammar::compile("r1 = { &\"ab\" ~ \"a\" ~ \"b\" }").unwrap();
    let mut pairs = parse_pairs(&g, 1, "ab", to_rule).unwrap();
    let root = pairs.next().unwrap();
    assert_eq!(root.as_span().start(), 0);
    assert_eq!(root.as_span().end(), 2);
}

#[test]
fn test_negative_lookahead_rejects() {
    let g = Grammar::compile("r1 = { !\"a\" ~ \"b\" }").unwrap();
    let res = parse_pairs(&g, 1, "ab", to_rule);
    assert!(res.is_err(), "should fail since !\"a\" rejects 'a' prefix");
}

#[test]
fn test_negative_lookahead_accepts() {
    let g = Grammar::compile("r1 = { !\"a\" ~ \"b\" }").unwrap();
    let mut pairs = parse_pairs(&g, 1, "b", to_rule).unwrap();
    let root = pairs.next().unwrap();
    assert_eq!(root.as_str(), "b");
}

#[test]
fn test_repetition_bounds_m_n() {
    // e{2,4}
    let g = Grammar::compile("r1 = { \"a\"{2,4} }").unwrap();
    assert!(parse_pairs(&g, 1, "a", to_rule).is_err());
    assert!(parse_pairs(&g, 1, "aa", to_rule).is_ok());
    let mut p = parse_pairs(&g, 1, "aaaa", to_rule).unwrap();
    let root = p.next().unwrap();
    assert_eq!(root.as_str(), "aaaa");
    // 5 a's: only first 4 consumed (r1 rule only spans 4, but EOI would fail since not
    // used here). Just check as_str for a 5-char input parses first 4.
    let mut p2 = parse_pairs(&g, 1, "aaaaa", to_rule).unwrap();
    let root2 = p2.next().unwrap();
    assert_eq!(root2.as_str(), "aaaa");
}

#[test]
fn test_repetition_bounds_exactly_n() {
    let g = Grammar::compile("r1 = { \"a\"{3} }").unwrap();
    assert!(parse_pairs(&g, 1, "aa", to_rule).is_err());
    let mut p = parse_pairs(&g, 1, "aaa", to_rule).unwrap();
    assert_eq!(p.next().unwrap().as_str(), "aaa");
}

#[test]
fn test_repetition_bounds_n_or_more() {
    let g = Grammar::compile("r1 = { \"a\"{2,} }").unwrap();
    assert!(parse_pairs(&g, 1, "a", to_rule).is_err());
    let mut p = parse_pairs(&g, 1, "aaaaa", to_rule).unwrap();
    assert_eq!(p.next().unwrap().as_str(), "aaaaa");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
enum RuleWs {
    EOI = 0,
    WHITESPACE = 1,
    r1 = 2,
    inner = 3,
}
fn to_rule_ws(idx: usize) -> RuleWs {
    match idx {
        0 => RuleWs::EOI,
        1 => RuleWs::WHITESPACE,
        2 => RuleWs::r1,
        3 => RuleWs::inner,
        _ => unreachable!(),
    }
}

#[test]
fn test_compound_atomic_dollar() {
    // $ = CompoundAtomic: still emits inner pairs but no whitespace skipping inside.
    let g = Grammar::compile(
        "WHITESPACE = _{ \" \" }\nr1 = ${ inner ~ inner }\ninner = { \"a\" }",
    )
    .unwrap();
    let r1_idx = g.rule_index("r1").unwrap();
    // Since $ suppresses internal whitespace skipping, "a a" (with space) should fail
    let res_with_space = parse_pairs(&g, r1_idx, "a a", to_rule_ws);
    assert!(res_with_space.is_err(), "compound atomic should not skip whitespace internally");
    // But without space it should succeed and still emit `inner` children
    let mut p = parse_pairs(&g, r1_idx, "aa", to_rule_ws).unwrap();
    let root = p.next().unwrap();
    let children: Vec<_> = root.into_inner().collect();
    assert_eq!(children.len(), 2, "compound atomic should still emit inner pairs");
}

#[test]
fn test_tag_extraction() {
    let g = Grammar::compile("r1 = { #tag = inner }\ninner = { \"x\" }").unwrap();
    let mut p = parse_pairs(&g, 1, "x", to_rule).unwrap();
    let root = p.next().unwrap();
    let tagged = root.into_inner().find_first_tagged("tag");
    assert!(tagged.is_some(), "expected to find pair tagged 'tag'");
    assert_eq!(tagged.unwrap().as_str(), "x");
}

#[test]
fn test_bare_rule_alias_is_deterministic() {
    // Regression: a rule body that is a single bare reference to another rule
    // (`x = { y }`, no `~`/`|`/modifiers wrapping it) compiles straight through
    // `compile_ident` to the target rule's slot. `Grammar::compile` iterates its
    // internal `reachable: HashSet<(String, Ctx)>` in unspecified order (Rust's
    // default hasher reseeds per collection), so naively copying the target
    // slot's contents at that point could read a not-yet-backpatched placeholder
    // depending on iteration order — passing on some runs, silently producing an
    // empty/wrong match on others. Compile repeatedly to catch any reintroduction
    // of that non-determinism.
    for _ in 0..50 {
        let g = Grammar::compile("x = { y }\ny = { \"z\" }").unwrap();
        let x_idx = g.rule_index("x").unwrap();
        let mut p = parse_pairs(&g, x_idx, "z", to_rule).unwrap();
        let root = p.next().unwrap();
        assert_eq!(root.as_str(), "z");
        // The alias must still produce the nested `y` pair (pest-equivalent
        // behavior: aliasing does not suppress the aliased rule's own pair).
        let inner: Vec<_> = root.into_inner().collect();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].as_str(), "z");
    }
}

#[test]
fn test_tagged_rule_reference_is_deterministic() {
    // Regression: `#tag = rule_ref` had the identical premature-copy bug as the
    // bare-alias case above (see `test_bare_rule_alias_is_deterministic`).
    for _ in 0..50 {
        let g = Grammar::compile("r1 = { #tag = inner }\ninner = { \"x\" }").unwrap();
        let mut p = parse_pairs(&g, 1, "x", to_rule).unwrap();
        let root = p.next().unwrap();
        let tagged = root.into_inner().find_first_tagged("tag");
        assert!(tagged.is_some(), "expected to find pair tagged 'tag'");
        assert_eq!(tagged.unwrap().as_str(), "x");
    }
}
