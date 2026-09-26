use pika::Grammar;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum TestRule {
    EOI = 0,
    P = 1,
}

#[test]
fn test_parse_memo_basic() {
    let grammar = Grammar::compile("P = { \"a\" ~ \"b\" }").unwrap();
    let table = pika::parse_table(&grammar, 1, "ab", |idx| {
        match idx {
            0 => TestRule::EOI,
            _ => TestRule::P,
        }
    });
    assert_eq!(table.input(), "ab");
}
