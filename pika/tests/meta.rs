use pika::Grammar;

#[test]
fn test_grammar_compile_dial() {
    let grammar = Grammar::compile(include_str!("../../src/grammar.pika"))
        .expect("compilation of dial grammar should succeed");
    let names = grammar.rule_names();
    assert!(names.contains(&"file".to_string()));
    assert!(names.contains(&"main".to_string()));
    assert!(names.contains(&"blockless_expression".to_string()));
    assert!(names.contains(&"WHITESPACE".to_string()));
    assert!(names.contains(&"COMMENT".to_string()));
    assert_eq!(names[0], "EOI");
}
