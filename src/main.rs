extern crate pest;
#[macro_use]
extern crate pest_derive;

use std::fs::File;
use std::io::Read;
use std::path::Path;
use pest::Parser;
use pest_derive::Parser;


// #[cfg(debug_assertions)]
// const _GRAMMAR: &'static str = include_str!("grammar.pest"); // relative to this file

#[derive(Parser)]
#[grammar = "grammar.pest"] // relative to src
struct DialParser;


fn main() {
    let path = Path::new("test/gemini_generated.dial");

    println!("Test file: {}", path.canonicalize().unwrap().display());

    let mut file = File::open(path).unwrap();

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    // parse the file
    let _pairs = DialParser::parse(Rule::file, &contents).unwrap();
}
