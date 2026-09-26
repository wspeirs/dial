use pika::Parser;
use pika_derive::Parser;

#[derive(Parser)]
#[grammar_inline = "file = { SOI ~ \"hello\" ~ EOI }"]
struct HelloParser;

#[test]
fn test_derive_inline() {
    let pairs = HelloParser::parse(Rule::file, "hello").expect("should parse");
    assert_eq!(pairs.len(), 1);
    assert_eq!(Rule::file.name(), "file");
    assert_eq!(Rule::file.index(), 1);
    assert_eq!(Rule::from_index(1), Rule::file);
}
