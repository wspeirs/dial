use std::fs;
use std::path::Path;
use pika::Parser;
use pika_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pika"]
struct DialParser;

fn main() {
    let path = Path::new("test/gemini_generated.dial");
    println!("Test file: {}", path.canonicalize().unwrap().display());
    let contents = fs::read_to_string(path).unwrap();
    match DialParser::parse(Rule::file, &contents) {
        Ok(pairs) => println!("parsed {} top-level pairs", pairs.count()),
        Err(e) => println!("{e}"),
    }
}
