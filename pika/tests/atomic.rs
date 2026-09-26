use pika::{parse_pairs, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
enum Rule {
    EOI = 0,
    WHITESPACE = 1,
    ident = 2,
    pair_r = 3,
}

fn to_rule(idx: usize) -> Rule {
    match idx {
        0 => Rule::EOI,
        1 => Rule::WHITESPACE,
        2 => Rule::ident,
        3 => Rule::pair_r,
        _ => unreachable!(),
    }
}

#[test]
fn test_atomic_and_whitespace() {
    let grammar_src = r#"
WHITESPACE = _{ " " }
ident = @{ ASCII_ALPHA ~ (ASCII_ALPHA | ASCII_DIGIT)* }
pair_r = { ident ~ "=" ~ ident }
"#;
    let grammar = Grammar::compile(grammar_src).expect("grammar should compile");
    let input = "foo = bar1";
    let mut pairs = parse_pairs(&grammar, 3, input, to_rule).expect("should parse");

    assert_eq!(pairs.len(), 1);
    let root = pairs.next().unwrap();
    assert_eq!(root.as_rule(), Rule::pair_r);

    let idents: Vec<_> = root.into_inner().collect();
    assert_eq!(idents.len(), 2);
    assert_eq!(idents[0].as_rule(), Rule::ident);
    assert_eq!(idents[0].as_str(), "foo");
    assert_eq!(idents[0].clone().into_inner().count(), 0);

    assert_eq!(idents[1].as_rule(), Rule::ident);
    assert_eq!(idents[1].as_str(), "bar1");
    assert_eq!(idents[1].clone().into_inner().count(), 0);
}
