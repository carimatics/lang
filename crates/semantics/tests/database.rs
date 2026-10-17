use lang_diagnostics::codes;
use lang_query::GraphError;
use lang_semantics::database::{AnalysisStatus, Database, DbError, LimitKind, Limits};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "lang-db-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(fs::canonicalize(p).unwrap())
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.file(name), text).unwrap();
    }
    fn db(&self) -> Database {
        Database::new(Some(&self.0), Limits::default()).unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn open(db: &mut Database, path: &Path, text: &str) -> PathBuf {
    db.open(path, Arc::from(text.as_bytes())).unwrap()
}
const MAIN: &str = "mod math; fn main() -> i64 { return math::value(); }";
const CHILD: &str = "pub fn value() -> i64 { return 42; }";
const BAD_CHILD: &str = "pub fn value() -> i64 { return false; }";

#[cfg(unix)]
#[test]
fn authorized_overlay_survives_disk_replacement_without_acquiring_new_authority() {
    let root = Directory::new();
    let outside = Directory::new();
    root.write("main.lang", "fn main() -> i64 { return 1; }");
    outside.write("secret.lang", "fn main() -> i64 { return false; }");
    let mut db = root.db();
    let path = open(
        &mut db,
        &root.file("main.lang"),
        "fn main() -> i64 { return 2; }",
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(db.analyze(&path).unwrap().status, AnalysisStatus::Valid);
    std::os::unix::fs::symlink(outside.file("secret.lang"), &path).unwrap();
    assert_eq!(db.analyze(&path).unwrap().status, AnalysisStatus::Valid);
    assert_eq!(db.stats().disk_reads, 0);
    db.close(&path).unwrap();
    assert!(matches!(db.parsed(&path), Err(DbError::OutsideRoot(_))));
    assert_eq!(db.stats().disk_reads, 0);
}

#[test]
fn public_queries_validate_source_authority_before_reading_or_caching() {
    let d = Directory::new();
    let outside = Directory::new();
    outside.write("outside.lang", "fn outside() -> i64 { return 7; }");
    let traversal =
        d.0.join("..")
            .join(outside.0.file_name().unwrap())
            .join("outside.lang");
    let mut db = d.db();
    assert!(db.parsed(&traversal).is_err());
    assert_eq!(db.stats().disk_reads, 0);
    assert!(db.raw_input(&traversal).is_none());
    assert!(db.modules(&traversal).is_err());
    assert!(db.prepare(&traversal).is_err());
    assert!(db.analyze(&traversal).is_err());
    assert_eq!(db.stats().disk_reads, 0);
}

#[test]
fn rejected_open_does_not_consume_the_file_budget() {
    let d = Directory::new();
    let mut db = Database::new(
        Some(&d.0),
        Limits {
            files: 1,
            input_bytes: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        db.open(&d.file("too-big.lang"), Arc::from(&b"ab"[..])),
        Err(DbError::Limit(LimitKind::InputBytes))
    ));
    let path = open(&mut db, &d.file("valid.lang"), "");
    assert_eq!(db.analyze(&path).unwrap().status, AnalysisStatus::Valid);
}

#[cfg(unix)]
#[test]
fn public_parse_rejects_symlink_escape_and_reuses_canonical_aliases() {
    let d = Directory::new();
    let outside = Directory::new();
    outside.write("outside.lang", "fn outside() -> i64 { return 7; }");
    std::os::unix::fs::symlink(outside.file("outside.lang"), d.file("escape.lang")).unwrap();
    d.write("source.lang", "fn f() -> i64 { return 1; }");
    std::os::unix::fs::symlink(d.file("source.lang"), d.file("alias.lang")).unwrap();
    let mut db = d.db();
    assert!(matches!(
        db.parsed(&d.file("escape.lang")),
        Err(DbError::OutsideRoot(_))
    ));
    assert_eq!(db.stats().disk_reads, 0);
    let first = db.parsed(&d.file("alias.lang")).unwrap();
    assert!(Arc::ptr_eq(
        &first,
        &db.parsed(&d.file("source.lang")).unwrap()
    ));
    assert_eq!(db.stats().disk_reads, 1);
    let path = open(
        &mut db,
        &d.file("source.lang"),
        "fn f() -> i64 { return 2; }",
    );
    let current = db.parsed(&path).unwrap();
    assert!(!Arc::ptr_eq(&first, &current));
    assert!(Arc::ptr_eq(
        &current,
        &db.parsed(&d.file("alias.lang")).unwrap()
    ));
}

#[test]
fn real_parser_module_and_semantic_queries_reuse_cache_and_invalidate_only_consumers() {
    let d = Directory::new();
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    let child = open(&mut db, &d.file("math.lang"), CHILD);
    let other = open(
        &mut db,
        &d.file("other.lang"),
        "fn other() -> i64 { return 7; }",
    );
    let parsed = db.parsed(&main).unwrap();
    let modules = db.modules(&main).unwrap();
    assert!(std::ptr::eq(
        modules.graph.as_ref().unwrap().modules()[0].tree(),
        parsed.result.as_ref().unwrap().as_ref()
    ));
    let first = db.analyze(&main).unwrap();
    let unrelated = db.analyze(&other).unwrap();
    assert_eq!(first.status, AnalysisStatus::Valid);
    let stats = db.stats();
    assert!(Arc::ptr_eq(&first, &db.analyze(&main).unwrap()));
    assert_eq!(stats, db.stats());
    db.change(&child, Arc::from(BAD_CHILD.as_bytes())).unwrap();
    assert!(Arc::ptr_eq(&unrelated, &db.analyze(&other).unwrap()));
    assert!(Arc::ptr_eq(&parsed, &db.parsed(&main).unwrap()));
    let changed = db.analyze(&main).unwrap();
    assert_eq!(changed.status, AnalysisStatus::Diagnostics);
    assert!(
        changed
            .diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_I64)
    );
    assert_eq!(db.stats().parses, stats.parses + 1);
    assert_eq!(db.stats().disk_reads, 0);
    for diagnostic in &changed.diagnostics {
        diagnostic.validate(&changed.sources).unwrap();
        assert_eq!(changed.paths[&diagnostic.primary.span.source()], child);
    }
    assert_eq!(first.status, AnalysisStatus::Valid);
    assert!(
        !first
            .diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_I64)
    );
}

#[test]
fn edited_closed_reopened_or_superseded_jobs_never_replace_current_results() {
    let d = Directory::new();
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), "fn f() -> i64 { return 1; }");
    let old = db.prepare(&main).unwrap();
    db.change(&main, Arc::from(&b"fn f() -> i64 { return 2; }"[..]))
        .unwrap();
    let current = db.analyze(&main).unwrap();
    assert!(matches!(
        db.publish(old.compute()),
        Err(DbError::Graph(GraphError::StaleTicket))
    ));
    assert!(Arc::ptr_eq(&current, &db.analyze(&main).unwrap()));
    let old = db.prepare(&main).unwrap();
    db.close(&main).unwrap();
    open(&mut db, &main, "fn f() -> i64 { return 3; }");
    let current = db.analyze(&main).unwrap();
    assert!(matches!(
        db.publish(old.compute()),
        Err(DbError::Graph(GraphError::StaleTicket))
    ));
    assert!(Arc::ptr_eq(&current, &db.analyze(&main).unwrap()));
    let old = db.prepare(&main).unwrap();
    let newer = db.prepare(&main).unwrap();
    db.publish(newer.compute()).unwrap();
    assert!(matches!(
        db.publish(old.compute()),
        Err(DbError::Graph(GraphError::StaleTicket))
    ));
}

#[test]
fn incomplete_source_diagnostics_share_original_snapshots_and_clear_after_edit() {
    let d = Directory::new();
    let mut db = d.db();
    let path = open(&mut db, &d.file("main.lang"), "// 😀\nfn f( {");
    let old = db.analyze(&path).unwrap();
    assert_eq!(old.status, AnalysisStatus::Diagnostics);
    assert!(
        old.diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_SYNTAX)
    );
    for diagnostic in &old.diagnostics {
        diagnostic.validate(&old.sources).unwrap();
        assert_eq!(old.paths[&diagnostic.primary.span.source()], path);
    }
    db.change(&path, Arc::from(&b"fn f() -> i64 { return 1; }"[..]))
        .unwrap();
    let new = db.analyze(&path).unwrap();
    assert_eq!(new.status, AnalysisStatus::Valid);
    assert!(new.diagnostics.is_empty());
    assert!(!old.diagnostics.is_empty());
}

#[test]
fn close_discards_overlay_and_parent_queries_revert_to_bounded_disk_input() {
    let d = Directory::new();
    d.write("math.lang", CHILD);
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    let child = open(&mut db, &d.file("math.lang"), BAD_CHILD);
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    db.close(&child).unwrap();
    let after = db.analyze(&main).unwrap();
    assert_eq!(after.status, AnalysisStatus::Valid);
    assert_eq!(db.stats().disk_reads, 1);
    assert_eq!(db.open_paths().count(), 1);
    db.close(&main).unwrap();
    assert_eq!(db.open_paths().count(), 0);
}

#[test]
fn missing_files_recover_on_watch_and_unrelated_parser_snapshots_remain_cached() {
    let d = Directory::new();
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    let other = open(
        &mut db,
        &d.file("other.lang"),
        "fn other() -> i64 { return 1; }",
    );
    let unrelated = db.parsed(&other).unwrap();
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    d.write("math.lang", CHILD);
    db.refresh(&d.file("math.lang")).unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    d.write("math.lang", BAD_CHILD);
    db.refresh(&d.file("math.lang")).unwrap();
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    assert!(Arc::ptr_eq(&unrelated, &db.parsed(&other).unwrap()));
    assert_eq!(db.stats().disk_reads, 2);
}

#[test]
fn missing_virtual_children_are_discovered_again_after_close_and_reopen() {
    let d = Directory::new();
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    let child = open(&mut db, &d.file("math.lang"), CHILD);
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    assert_eq!(db.stats().disk_reads, 0);
    assert!(!child.exists());
    db.close(&child).unwrap();
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    open(&mut db, &child, CHILD);
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
}

#[test]
fn no_root_means_no_file_reads_and_explicit_roots_reject_outside_documents() {
    let d = Directory::new();
    d.write("math.lang", CHILD);
    let mut db = Database::new(None, Limits::default()).unwrap();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    assert_eq!(
        db.analyze(&main).unwrap().status,
        AnalysisStatus::Diagnostics
    );
    assert_eq!(db.stats().disk_reads, 0);
    let other = Directory::new();
    let mut db = d.db();
    assert!(matches!(
        db.open(&other.file("outside.lang"), Arc::from(&b""[..])),
        Err(DbError::OutsideRoot(_))
    ));
    assert_eq!(db.open_paths().count(), 0);
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected_before_any_outside_source_bytes_are_read() {
    let d = Directory::new();
    let outside = Directory::new();
    outside.write("outside.lang", CHILD);
    std::os::unix::fs::symlink(outside.file("outside.lang"), d.file("math.lang")).unwrap();
    let mut db = d.db();
    let main = open(&mut db, &d.file("main.lang"), MAIN);
    let analysis = db.analyze(&main).unwrap();
    assert_eq!(analysis.status, AnalysisStatus::Diagnostics);
    assert_eq!(db.stats().disk_reads, 0);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MODULE_ERROR)
    );
}

#[test]
fn host_cache_input_and_graph_limits_do_not_publish_partial_success() {
    let d = Directory::new();
    let limits = Limits {
        files: 1,
        ..Limits::default()
    };
    let mut db = Database::new(None, limits).unwrap();
    let main = open(&mut db, &d.file("main.lang"), " ");
    assert!(matches!(
        db.open(&d.file("other.lang"), Arc::from(&b" "[..])),
        Err(DbError::Limit(LimitKind::Files))
    ));
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    let limits = Limits {
        input_bytes: 1,
        ..Limits::default()
    };
    let mut db = Database::new(None, limits).unwrap();
    open(&mut db, &d.file("a.lang"), " ");
    assert!(matches!(
        db.open(&d.file("b.lang"), Arc::from(&b" "[..])),
        Err(DbError::Limit(LimitKind::InputBytes))
    ));
    assert_eq!(db.open_paths().count(), 1);
    let mut limits = Limits::default();
    limits.graph.nodes = 5;
    let mut db = Database::new(None, limits).unwrap();
    assert!(matches!(
        db.open(&d.file("main.lang"), Arc::from(&b""[..])),
        Err(DbError::Graph(GraphError::LimitExceeded(_)))
    ));
    assert_eq!(db.open_paths().count(), 0);
}

#[test]
fn parser_and_module_cache_limits_recover_after_the_input_becomes_small() {
    let d = Directory::new();
    let limits = Limits {
        parse_nodes: 1,
        ..Limits::default()
    };
    let mut db = Database::new(None, limits).unwrap();
    let main = open(&mut db, &d.file("main.lang"), "fn f() -> i64 { return 1; }");
    let error = db.analyze(&main).unwrap();
    assert_eq!(error.status, AnalysisStatus::Diagnostics);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|d| d.code == codes::RESOURCE_LIMIT)
    );
    for d in &error.diagnostics {
        d.validate(&error.sources).unwrap();
    }
    db.change(&main, Arc::from(&b""[..])).unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    let limits = Limits {
        module_nodes: 1,
        ..Limits::default()
    };
    let mut db = Database::new(None, limits).unwrap();
    let main = open(
        &mut db,
        &d.file("module.lang"),
        "fn f() -> i64 { return 1; }",
    );
    assert!(matches!(
        db.modules(&main),
        Err(DbError::Limit(LimitKind::ModuleNodes))
    ));
    db.change(&main, Arc::from(&b""[..])).unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
}

#[test]
fn ownership_type_limit_is_a_revision_safe_source_diagnostic() {
    let d = Directory::new();
    let mut limits = Limits::default();
    limits.typing.types.work = 0;
    let mut db = Database::new(None, limits).unwrap();
    let main = open(
        &mut db,
        &d.file("main.lang"),
        "fn f(x: &i64) -> i64 { return 0; }",
    );
    let snapshot = db.analyze(&main).unwrap();
    assert_eq!(snapshot.status, AnalysisStatus::Diagnostics);
    assert!(snapshot.error.is_none());
    assert_eq!(snapshot.diagnostics[0].code, codes::RESOURCE_LIMIT);
    snapshot.diagnostics[0].validate(&snapshot.sources).unwrap();
    db.change(&main, Arc::from(&b"fn f() -> i64 { return 0; }"[..]))
        .unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
}

#[test]
fn ownership_body_type_diagnostics_are_cached_and_invalidated_with_source_revisions() {
    let d = Directory::new();
    let mut db = Database::new(None, Default::default()).unwrap();
    let main = open(
        &mut db,
        &d.file("main.lang"),
        "fn main() -> i64 { let mut value: core::Option<i64> = core::Option::Some(true); return 0; }",
    );
    let snapshot = db.analyze(&main).unwrap();
    assert_eq!(snapshot.status, AnalysisStatus::Diagnostics);
    assert!(snapshot.error.is_none());
    let diagnostic = snapshot
        .diagnostics
        .iter()
        .find(|d| d.code == codes::TYPE_MISMATCH)
        .unwrap();
    assert_eq!(
        snapshot.sources.slice(diagnostic.primary.span).unwrap(),
        "true"
    );
    diagnostic.validate(&snapshot.sources).unwrap();
    db.change(&main, Arc::from(&b"fn main() -> i64 { return 0; }"[..]))
        .unwrap();
    let latest = db.analyze(&main).unwrap();
    assert_eq!(latest.status, AnalysisStatus::Valid);
    assert!(latest.diagnostics.is_empty());
    assert_eq!(
        snapshot.sources.slice(diagnostic.primary.span).unwrap(),
        "true"
    );
}

#[test]
fn move_diagnostics_preserve_origin_spans_and_recover_after_revision_changes() {
    let d = Directory::new();
    let mut db = Database::new(None, Default::default()).unwrap();
    let text = "fn bad(text: core::Text) -> i64 { let mut moved: core::Text = text; return core::text_len(text); }";
    let main = open(&mut db, &d.file("main.lang"), text);
    let snapshot = db.analyze(&main).unwrap();
    assert_eq!(snapshot.status, AnalysisStatus::Diagnostics);
    assert!(snapshot.error.is_none());
    let d = snapshot
        .diagnostics
        .iter()
        .find(|d| d.code == codes::INVALID_OWNER_USE)
        .unwrap();
    assert_eq!(snapshot.sources.slice(d.primary.span).unwrap(), "text");
    assert_eq!(snapshot.sources.slice(d.secondary[0].span).unwrap(), "text");
    assert!(d.secondary[0].span.range().start < d.primary.span.range().start);
    db.change(&main, Arc::from(&b"fn main() -> i64 { return 0; }"[..]))
        .unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    assert_eq!(snapshot.sources.slice(d.primary.span).unwrap(), "text");
}

#[test]
fn loan_diagnostics_preserve_origins_and_recover_after_revision_changes() {
    let directory = Directory::new();
    let mut db = Database::new(None, Default::default()).unwrap();
    let text = "fn bad(text: core::Text) -> core::Text { let mut read: &core::Text = &text; return text; }";
    let main = open(&mut db, &directory.file("main.lang"), text);
    let snapshot = db.analyze(&main).unwrap();
    assert_eq!(snapshot.status, AnalysisStatus::Diagnostics);
    assert!(snapshot.error.is_none());
    let d = snapshot
        .diagnostics
        .iter()
        .find(|d| d.code == codes::INVALID_LOAN)
        .unwrap();
    assert_eq!(snapshot.sources.slice(d.primary.span).unwrap(), "text");
    assert_eq!(
        snapshot.sources.slice(d.secondary[0].span).unwrap(),
        "&text"
    );
    assert!(d.secondary[0].span.range().start < d.primary.span.range().start);
    db.change(&main, Arc::from(&b"fn main() -> i64 { return 0; }"[..]))
        .unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
    assert_eq!(
        snapshot.sources.slice(d.secondary[0].span).unwrap(),
        "&text"
    );
}

#[test]
fn closing_a_document_releases_cache_capacity_and_retries_previous_resource_failures() {
    let d = Directory::new();
    let mut probe = Database::new(None, Limits::default()).unwrap();
    let a = open(
        &mut probe,
        &d.file("probe.lang"),
        "fn f() -> i64 { return 1; }",
    );
    let nodes = probe
        .parsed(&a)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
        .nodes()
        .len();
    let mut db = Database::new(
        None,
        Limits {
            parse_nodes: nodes,
            ..Limits::default()
        },
    )
    .unwrap();
    let a = open(&mut db, &d.file("a.lang"), "fn f() -> i64 { return 1; }");
    let b = open(&mut db, &d.file("b.lang"), "fn g() -> i64 { return 2; }");
    assert_eq!(db.analyze(&a).unwrap().status, AnalysisStatus::Valid);
    assert_eq!(db.analyze(&b).unwrap().status, AnalysisStatus::Diagnostics);
    db.close(&a).unwrap();
    assert_eq!(db.analyze(&b).unwrap().status, AnalysisStatus::Valid);
}

#[test]
fn diagnostic_cache_limits_refuse_publication_and_clean_edits_can_recover() {
    let d = Directory::new();
    let mut db = Database::new(
        None,
        Limits {
            diagnostic_bytes: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let main = open(
        &mut db,
        &d.file("main.lang"),
        "fn f() -> i64 { return false; }",
    );
    assert!(matches!(
        db.analyze(&main),
        Err(DbError::Limit(LimitKind::DiagnosticBytes))
    ));
    db.change(&main, Arc::from(&b"fn f() -> i64 { return 1; }"[..]))
        .unwrap();
    assert_eq!(db.analyze(&main).unwrap().status, AnalysisStatus::Valid);
}
