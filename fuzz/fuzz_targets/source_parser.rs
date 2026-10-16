#![no_main]

use lang_diagnostics::SourceMap;
use lang_syntax::{LexLimits, ParseError, ParseLimits, add_source, parse, parse_expression};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let mut sources = SourceMap::new();
    let lex = LexLimits {
        source_bytes: 16_384,
        tokens: 8_192,
        diagnostics: 256,
        ..LexLimits::default()
    };
    let Ok(source) = add_source(&mut sources, "fuzz.lang", bytes, lex) else {
        return;
    };
    let limits = ParseLimits {
        lex,
        nodes: 16_384,
        diagnostics: 256,
        work: 100_000,
        ..ParseLimits::default()
    };
    for result in [
        parse(&sources, source, limits),
        parse_expression(&sources, source, limits),
    ] {
        match result {
            Ok(tree) => {
                assert_eq!(tree.reconstruct(&sources).unwrap().as_bytes(), bytes);
                for diagnostic in tree.diagnostics() {
                    diagnostic.validate(&sources).unwrap();
                }
            }
            Err(ParseError::Limit(diagnostic)) => {
                diagnostic.validate(&sources).unwrap();
            }
            Err(ParseError::Lex(lang_syntax::LexError::Limit(diagnostic))) => {
                diagnostic.validate(&sources).unwrap();
            }
            Err(error) => panic!("unexpected source error: {error}"),
        }
    }
});
