use lang_diagnostics::{SourceMap, codes};
use lang_syntax::{InputErrorKind, LexError, LexLimits, Lexed, TokenKind as K, add_source, lex};

fn tokenize(text: &str) -> (SourceMap, Lexed) {
    let mut sources = SourceMap::new();
    let source = add_source(
        &mut sources,
        "test.lang",
        text.as_bytes(),
        LexLimits::default(),
    )
    .unwrap();
    let lexed = lex(&sources, source, LexLimits::default()).unwrap();
    assert_lossless(&sources, &lexed, text);
    (sources, lexed)
}

fn assert_lossless(sources: &SourceMap, lexed: &Lexed, text: &str) {
    let mut offset = 0;
    let mut reconstructed = String::new();
    for token in &lexed.tokens {
        assert_eq!(token.span.source(), lexed.source);
        assert_eq!(token.span.range().start, offset);
        assert!(token.span.range().end > offset);
        offset = token.span.range().end;
        reconstructed.push_str(sources.slice(token.span).unwrap());
    }
    assert_eq!(offset, text.len());
    assert_eq!(reconstructed, text);
    for diagnostic in &lexed.diagnostics {
        diagnostic.validate(sources).unwrap();
    }
}

fn significant(lexed: &Lexed) -> Vec<K> {
    lexed
        .tokens
        .iter()
        .map(|token| token.kind)
        .filter(|kind| !kind.is_trivia())
        .collect()
}

#[test]
fn keywords_are_exact_ascii_and_wildcard_is_distinct() {
    let (_, lexed) = tokenize(
        "fn let mut i64 bool if else while return mod use pub root extern struct enum match true false _ _x a1 Fn true1 i64x",
    );
    assert_eq!(
        significant(&lexed),
        [
            K::Fn,
            K::Let,
            K::Mut,
            K::I64,
            K::Bool,
            K::If,
            K::Else,
            K::While,
            K::Return,
            K::Mod,
            K::Use,
            K::Pub,
            K::Root,
            K::Extern,
            K::Struct,
            K::Enum,
            K::Match,
            K::True,
            K::False,
            K::Wildcard,
            K::Identifier,
            K::Identifier,
            K::Identifier,
            K::Identifier,
            K::Identifier
        ]
    );
    assert!(lexed.diagnostics.is_empty());
}

#[test]
fn ownership_punctuation_keeps_individual_closers_and_exact_keywords() {
    let (_, lexed) =
        tokenize("shared weak arena &mut *x core::Vec<core::Vec<i64>> a<1 Shared weakly arena2");
    assert!(lexed.diagnostics.is_empty());
    let tokens = significant(&lexed);
    assert_eq!(
        &tokens[..7],
        &[
            K::Shared,
            K::Weak,
            K::Arena,
            K::Ampersand,
            K::Mut,
            K::Star,
            K::Identifier
        ]
    );
    assert_eq!(tokens.iter().filter(|k| **k == K::GreaterThan).count(), 2);
    assert_eq!(&tokens[tokens.len() - 3..], &[K::Identifier; 3]);
}

#[test]
fn punctuation_prefers_two_character_tokens() {
    let (_, lexed) = tokenize("(){}:;,.=+< -> :: => ::: ==> -->");
    assert_eq!(
        significant(&lexed),
        [
            K::LeftParen,
            K::RightParen,
            K::LeftBrace,
            K::RightBrace,
            K::Colon,
            K::Semicolon,
            K::Comma,
            K::Dot,
            K::Equal,
            K::Plus,
            K::LessThan,
            K::Arrow,
            K::ColonColon,
            K::FatArrow,
            K::ColonColon,
            K::Colon,
            K::Equal,
            K::FatArrow,
            K::Invalid,
            K::Arrow
        ]
    );
    assert_eq!(lexed.diagnostics.len(), 1);
}

#[test]
fn decimal_boundaries_and_adjacent_negative_literals() {
    let (_, lexed) =
        tokenize("0 -0 0009 -0009 9223372036854775807 -9223372036854775808 a+-1 a-1 - 1");
    assert_eq!(
        significant(&lexed),
        [
            K::Integer(0),
            K::Integer(0),
            K::Integer(9),
            K::Integer(-9),
            K::Integer(i64::MAX),
            K::Integer(i64::MIN),
            K::Identifier,
            K::Plus,
            K::Integer(-1),
            K::Identifier,
            K::Integer(-1),
            K::Invalid,
            K::Integer(1)
        ]
    );
    assert_eq!(lexed.diagnostics.len(), 1);
}

#[test]
fn overflow_and_malformed_tokens_keep_the_entire_range() {
    let (sources, lexed) =
        tokenize("9223372036854775808 -9223372036854775809 12abc 0xff 1_000 -3x 4é");
    assert_eq!(
        significant(&lexed),
        [
            K::IntegerOutOfRange,
            K::IntegerOutOfRange,
            K::Invalid,
            K::Invalid,
            K::Invalid,
            K::Invalid,
            K::Integer(4),
            K::Invalid
        ]
    );
    let labels: Vec<_> = lexed
        .diagnostics
        .iter()
        .map(|d| (d.code, sources.slice(d.primary.span).unwrap()))
        .collect();
    assert_eq!(
        labels,
        [
            (codes::INTEGER_OUT_OF_RANGE, "9223372036854775808"),
            (codes::INTEGER_OUT_OF_RANGE, "-9223372036854775809"),
            (codes::INVALID_TOKEN, "12abc"),
            (codes::INVALID_TOKEN, "0xff"),
            (codes::INVALID_TOKEN, "1_000"),
            (codes::INVALID_TOKEN, "-3x"),
            (codes::INVALID_TOKEN, "é")
        ]
    );
}

#[test]
fn unicode_is_allowed_in_comments_and_invalid_scalars_remain_separate() {
    let (sources, lexed) =
        tokenize(" \t\r\n// 日本😀\rstill comment\n/* é /* 内 */ 文 */ fn\u{a0}😀");
    assert_eq!(significant(&lexed), [K::Fn, K::Invalid, K::Invalid]);
    let invalid: Vec<_> = lexed
        .diagnostics
        .iter()
        .map(|d| sources.slice(d.primary.span).unwrap())
        .collect();
    assert_eq!(invalid, ["\u{a0}", "😀"]);
    let line = lexed
        .tokens
        .iter()
        .find(|t| t.kind == K::LineComment)
        .unwrap();
    assert_eq!(
        sources.slice(line.span).unwrap(),
        "// 日本😀\rstill comment"
    );
    assert_eq!(
        sources
            .file(lexed.source)
            .unwrap()
            .position(line.span.range().end)
            .unwrap()
            .line,
        2
    );
}

#[test]
fn nul_is_rejected_in_each_context_and_unterminated_comment_is_preserved() {
    let (sources, lexed) = tokenize("\0// a\0b\n/* 内\0 /* nested */");
    assert_eq!(
        lexed.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(),
        [
            codes::INVALID_TOKEN,
            codes::INVALID_TOKEN,
            codes::INVALID_TOKEN,
            codes::UNTERMINATED_COMMENT
        ]
    );
    assert_eq!(
        sources
            .slice(lexed.diagnostics.last().unwrap().primary.span)
            .unwrap(),
        "/* 内\0 /* nested */"
    );
    assert_eq!(lexed.tokens.last().unwrap().kind, K::BlockComment);
}

#[test]
fn invalid_encoding_has_raw_byte_ranges_and_creates_no_snapshot() {
    let mut sources = SourceMap::new();
    for (bytes, kind, range) in [
        (b"a\xffb".as_slice(), InputErrorKind::InvalidUtf8, 1..2),
        (b"a\xf0\x9f".as_slice(), InputErrorKind::InvalidUtf8, 1..3),
        (b"\xef\xbb\xbffn".as_slice(), InputErrorKind::Bom, 0..3),
    ] {
        let error = add_source(&mut sources, "bad.lang", bytes, LexLimits::default()).unwrap_err();
        assert_eq!(error.kind, kind);
        assert_eq!(error.byte_range, range);
        assert_eq!(error.code(), Some(codes::INVALID_ENCODING));
    }
    let source = add_source(&mut sources, "good.lang", b"fn", LexLimits::default()).unwrap();
    assert_eq!(sources.file(source).unwrap().text(), "fn");
}

#[test]
fn direct_snapshot_cannot_bypass_bom_rejection() {
    let mut sources = SourceMap::new();
    let source = sources.add("bad.lang", "\u{feff}fn").unwrap();
    let lexed = lex(&sources, source, LexLimits::default()).unwrap();
    assert_lossless(&sources, &lexed, "\u{feff}fn");
    assert_eq!(lexed.diagnostics[0].code, codes::INVALID_ENCODING);
    assert_eq!(lexed.diagnostics[0].primary.span.range(), 0..3);
}

fn limit_at(text: &str, limits: LexLimits, expected: std::ops::Range<usize>) {
    let mut sources = SourceMap::new();
    let source = sources.add("limit.lang", text).unwrap();
    let LexError::Limit(diagnostic) = lex(&sources, source, limits).unwrap_err() else {
        panic!("expected a resource diagnostic")
    };
    assert_eq!(diagnostic.code, codes::RESOURCE_LIMIT);
    assert_eq!(diagnostic.primary.span.range(), expected);
    diagnostic.validate(&sources).unwrap();
}

#[test]
fn byte_limits_apply_before_ingestion_and_to_direct_snapshots() {
    let limits = LexLimits {
        source_bytes: 2,
        ..LexLimits::default()
    };
    let mut sources = SourceMap::new();
    assert!(add_source(&mut sources, "exact.lang", b"fn", limits).is_ok());
    let error = add_source(&mut sources, "too-big.lang", b"abc", limits).unwrap_err();
    assert_eq!(error.kind, InputErrorKind::SourceBytes);
    assert_eq!(error.code(), Some(codes::RESOURCE_LIMIT));
    limit_at("😀", limits, 0..4);
}

#[test]
fn token_limits_count_trivia_and_allow_exact_boundary_and_empty_input() {
    let mut sources = SourceMap::new();
    let source = sources.add("empty", "").unwrap();
    assert!(
        lex(
            &sources,
            source,
            LexLimits {
                tokens: 0,
                ..LexLimits::default()
            }
        )
        .unwrap()
        .tokens
        .is_empty()
    );
    let source = sources.add("exact", "fn ").unwrap();
    assert_eq!(
        lex(
            &sources,
            source,
            LexLimits {
                tokens: 2,
                ..LexLimits::default()
            }
        )
        .unwrap()
        .tokens
        .len(),
        2
    );
    limit_at(
        "fn x",
        LexLimits {
            tokens: 2,
            ..LexLimits::default()
        },
        3..3,
    );
}

#[test]
fn nested_comment_limits_are_iterative_and_count_the_outer_comment() {
    let exact = format!("{}{}", "/*".repeat(256), "*/".repeat(256));
    let (_, lexed) = tokenize(&exact);
    assert!(lexed.diagnostics.is_empty());
    let excessive = format!("/*{exact}*/");
    limit_at(&excessive, LexLimits::default(), 512..514);
    limit_at(
        "/**/",
        LexLimits {
            comment_depth: 0,
            ..LexLimits::default()
        },
        0..2,
    );
}

#[test]
fn invalid_source_diagnostics_are_bounded() {
    limit_at(
        "😀?",
        LexLimits {
            diagnostics: 1,
            ..LexLimits::default()
        },
        4..5,
    );
    limit_at(
        "?",
        LexLimits {
            diagnostics: 0,
            ..LexLimits::default()
        },
        0..1,
    );
}

#[test]
fn deterministic_mixed_unicode_inputs_always_reconstruct_and_validate() {
    let alphabet = [
        "a", "0", "-", "/", "*", "\n", "\r", "😀", "é", "\0", "_", "+", " ", ":", "=", "\u{feff}",
    ];
    let mut state = 0x231f_6a73_u32;
    for _ in 0..512 {
        let mut input = String::new();
        for _ in 0..64 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            input.push_str(alphabet[(state >> 16) as usize % alphabet.len()]);
        }
        // Direct snapshots also exercise invalid leading BOM recovery.
        let mut sources = SourceMap::new();
        let source = sources.add("mixed", &input).unwrap();
        match lex(&sources, source, LexLimits::default()) {
            Ok(lexed) => assert_lossless(&sources, &lexed, &input),
            Err(LexError::Limit(diagnostic)) => {
                diagnostic.validate(&sources).unwrap();
            }
            Err(error) => panic!("unexpected lexer error: {error}"),
        }
    }
}
