#![no_main]

use lang_diagnostics::SourceMap;
use lang_syntax::{LexLimits, ParseError, ParseLimits, add_source, format, parse, parse_expression};
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
    for (expression,result) in [
        (false,parse(&sources, source, limits)),
        (true,parse_expression(&sources, source, limits)),
    ] {
        match result {
            Ok(tree) => {
                assert_eq!(tree.reconstruct(&sources).unwrap().as_bytes(), bytes);
                for diagnostic in tree.diagnostics() {
                    diagnostic.validate(&sources).unwrap();
                }
                let format_limits=format::Limits {work:1_000_000,output_bytes:65_536};
                if let Ok(output)=format::format(&tree,&sources,format_limits) {
                    if tree.has_errors() {assert_eq!(output.text.as_bytes(),bytes);}
                    else {
                        let formatted_source=sources.add("formatted.lang",&output.text).unwrap();
                        let mut parse_limits=limits;
                        parse_limits.lex.source_bytes=65_536;
                        parse_limits.lex.tokens=32_768;
                        parse_limits.work=1_000_000;
                        let formatted=if expression {parse_expression(&sources,formatted_source,parse_limits)}else{parse(&sources,formatted_source,parse_limits)}.unwrap();
                        assert!(!formatted.has_errors());
                        assert_eq!(tree.nodes().iter().map(|n|n.kind()).collect::<Vec<_>>(),formatted.nodes().iter().map(|n|n.kind()).collect::<Vec<_>>());
                        let original_tokens:Vec<_>=tree.tokens().iter().filter(|t|!t.kind.is_trivia()).map(|t|sources.slice(t.span).unwrap()).collect();
                        let new_tokens:Vec<_>=formatted.tokens().iter().filter(|t|!t.kind.is_trivia()).map(|t|sources.slice(t.span).unwrap()).collect();
                        assert_eq!(original_tokens,new_tokens);
                        assert_eq!(format::format(&formatted,&sources,format_limits).unwrap(),output);
                    }
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
