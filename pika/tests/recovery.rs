use pika::{parse_table, Grammar};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Rule {
    EOI = 0,
    Program = 1,
    Assign = 2,
    Var = 3,
    FnCall = 4,
    Params = 5,
    Param = 6,
}

fn to_rule(idx: usize) -> Rule {
    match idx {
        0 => Rule::EOI,
        1 => Rule::Program,
        2 => Rule::Assign,
        3 => Rule::Var,
        4 => Rule::FnCall,
        5 => Rule::Params,
        6 => Rule::Param,
        _ => unreachable!(),
    }
}

#[test]
fn test_paper_fig_5_recovery() {
    let grammar_src = r#"
Program = { Assign* }
Assign  = { Var ~ "=" ~ Param ~ ";" }
Var     = @{ ASCII_ALPHA_LOWER+ }
FnCall  = { Var ~ "(" ~ Params ~ ")" }
Params  = { Param ~ ("," ~ Param)* }
Param   = { FnCall | Var }
"#;
    let grammar = Grammar::compile(grammar_src).expect("grammar should compile");
    let input = "p=a;q=f(w);r=g(x;s=k(q,r);t=m(s,h(x));";

    let table = parse_table(&grammar, 1, input, to_rule);

    let gaps = table.unmatched_regions(&[Rule::Assign]);
    assert_eq!(gaps.len(), 1, "expected exactly 1 gap, got: {:?}", gaps);

    let gap = &gaps[0];
    assert_eq!(gap.as_str(), "r=g(x;");

    let next_match = table.next_match_after(Rule::Assign, gap.end());
    assert!(next_match.is_some(), "expected next match after gap");
    let pair = next_match.unwrap();
    assert_eq!(pair.as_rule(), Rule::Assign);
    assert_eq!(pair.as_str(), "s=k(q,r);");
}
