use lang::package::{doc, graph::Graph, source};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lang package doc {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let f = Self(root);
        f.write("Lang.json",r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}},"tests":["tests/not_run.lang"]}"#);
        f.write("helper/Lang.json",r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        f.write(
            "helper/src/main.lang",
            "/// 依存 type\npub struct Shared{value:i64}\n",
        );
        f.write("src/main.lang","mod helper;\n/// 日本語 <script>alert(1)</script> & \"quoted\"\npub struct Shared{value:i64}\n/// Convert dependency identity.\npub fn convert(value:helper::Shared)->helper::Shared{return value;}\npub fn nested(value:core::Option<Shared>)->i64{return 42;}\npub enum Event{Some(helper::Shared)}\nextern fn never_call(value:i64)->i64;\nfn main()->i64{return never_call(42);}\nfn hidden()->i64{return 0;}\n");
        f.write("tests/not_run.lang", "fn main()->i64{return 99;}");
        f
    }
    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn doc(&self, locked: bool) -> Output {
        let mut a = vec![
            "doc",
            "--target",
            "aarch64-apple-darwin",
            "--offline",
            "--message-format",
            "json",
        ];
        if locked {
            a.push("--locked");
        }
        self.run(&a)
    }
    fn artifact(&self, out: &Output) -> PathBuf {
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let j: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(j["schema"], 1);
        assert_eq!(j["kind"], "artifact");
        self.0.join(j["path"].as_str().unwrap())
    }
    fn analyzed(&self) -> lang::frontend::Analysis {
        let g = Graph::resolve(
            &self.0.join("Lang.json"),
            "aarch64-apple-darwin",
            Default::default(),
        )
        .unwrap();
        source::analyze(&g, Default::default()).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn failure(out: &Output, code: &str) {
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["code"], code);
}
fn fragments(html: &str) {
    let ids: Vec<_> = html
        .split("id=\"")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
        .collect();
    let set: BTreeSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), set.len());
    for target in html
        .split("href=\"#")
        .skip(1)
        .map(|s| s.split('"').next().unwrap())
    {
        assert!(set.contains(target), "missing {target}");
    }
}

#[test]
fn real_cli_emits_detached_escaped_api_with_resolved_dependency_and_nested_type_links() {
    let files = Files::new();
    let out = files.doc(false);
    let path = files.artifact(&out);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let html = fs::read_to_string(&path).unwrap();
    fragments(&html);
    assert!(html.contains("日本語 &lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;quoted&quot;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("Content-Security-Policy"));
    assert!(html.contains("data-start=\""));
    let analysis = files.analyzed();
    let p = analysis.result.unwrap();
    for function in p.source().functions.iter().filter(|f| !f.public) {
        assert!(!html.contains(&format!("<h2>{}</h2>", function.qualified_name)));
    }
    for function in p.source().functions.iter().filter(|f| f.public) {
        assert!(html.contains(&format!(
            "data-start=\"{}\" data-end=\"{}\"",
            function.signature_span.range().start,
            function.signature_span.range().end
        )));
    }
    let shared: Vec<_> = p
        .source()
        .definitions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.name.text == "Shared")
        .collect();
    assert_eq!(shared.len(), 2);
    for (id, d) in shared {
        assert!(html.contains(&format!("href=\"#t-{id}\"")));
        assert!(html.contains(&d.qualified_name));
    }
    let nested = p
        .source()
        .functions
        .iter()
        .enumerate()
        .find(|(_, f)| f.name.text == "nested")
        .unwrap();
    let section = html
        .split(&format!("<section id=\"f-{}\">", nested.0))
        .nth(1)
        .unwrap()
        .split("</section>")
        .next()
        .unwrap();
    assert!(section.contains("Referenced types:"));
    assert!(section.contains("href=\"#t-"));
    assert_eq!(
        doc::render(&p, &analysis.sources, Default::default()).unwrap(),
        html
    );
    files.artifact(&files.doc(true));
    let copy = files.0.join("detached.html");
    fs::copy(&path, &copy).unwrap();
    fs::remove_dir_all(files.0.join("src")).unwrap();
    fs::remove_dir_all(files.0.join("helper")).unwrap();
    assert_eq!(fs::read_to_string(copy).unwrap(), html);
}

#[test]
fn rendering_and_cli_output_are_deterministic_after_project_relocation() {
    let mut files = Files::new();
    let first = fs::read(files.artifact(&files.doc(false))).unwrap();
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let moved = files.0.with_extension("relocated");
    fs::rename(&files.0, &moved).unwrap();
    files.0 = moved;
    let second = fs::read(files.artifact(&files.doc(true))).unwrap();
    assert_eq!(first, second);
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
}

#[test]
fn stale_invalid_and_damaged_cached_outputs_preserve_previous_state() {
    for case in ["stale", "invalid", "cache"] {
        let files = Files::new();
        let path = files.artifact(&files.doc(false));
        let old = fs::read(&path).unwrap();
        let lock = fs::read(files.0.join("Lang.lock")).unwrap();
        if case == "cache" {
            fs::write(&path, b"damaged").unwrap();
            failure(&files.doc(true), "PKG0007");
            assert_eq!(fs::read(&path).unwrap(), b"damaged");
        } else {
            let source = fs::read_to_string(files.0.join("src/main.lang")).unwrap();
            files.write(
                "src/main.lang",
                format!(
                    "{source}\n{}",
                    if case == "invalid" {
                        "fn broken( {"
                    } else {
                        "// changed"
                    }
                ),
            );
            let out = files.doc(case == "stale");
            assert_eq!(out.status.code(), Some(1));
            assert!(out.stdout.is_empty());
            if case == "stale" {
                failure(&out, "PKG0007");
            }
            assert_eq!(fs::read(&path).unwrap(), old);
        }
        assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
    }
}

#[test]
fn exact_render_bytes_and_library_without_main_are_supported_but_limits_and_options_refuse() {
    let files = Files::new();
    files.write(
        "src/main.lang",
        "/// Library only.\npub fn answer()->i64{return 42;}\n",
    );
    let out = files.doc(false);
    let html = fs::read(files.artifact(&out)).unwrap();
    let a = files.analyzed();
    let p = a.result.unwrap();
    let limits = doc::Limits {
        bytes: html.len(),
        work: usize::MAX,
    };
    assert_eq!(
        doc::render(&p, &a.sources, limits).unwrap().as_bytes(),
        html
    );
    assert_eq!(
        doc::render(
            &p,
            &a.sources,
            doc::Limits {
                bytes: html.len() - 1,
                ..limits
            }
        )
        .unwrap_err()
        .code,
        "PKG0004"
    );
    assert_eq!(
        doc::render(&p, &a.sources, doc::Limits { work: 0, ..limits })
            .unwrap_err()
            .code,
        "PKG0004"
    );
    assert_eq!(
        files.run(&["doc", "--deny-warnings"]).status.code(),
        Some(2)
    );
    assert_eq!(files.run(&["doc", "--check"]).status.code(), Some(2));
}
#[cfg(unix)]
#[test]
fn generation_io_failure_keeps_the_committed_lock_and_document() {
    use std::os::unix::fs::PermissionsExt;
    let files = Files::new();
    let path = files.artifact(&files.doc(false));
    let old = fs::read(&path).unwrap();
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let source = fs::read_to_string(files.0.join("src/main.lang")).unwrap();
    files.write("src/main.lang", format!("{source}\n// new generation\n"));
    let directory = files.0.join("target/lang");
    let permissions = fs::metadata(&directory).unwrap().permissions();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o555)).unwrap();
    let out = files.doc(false);
    fs::set_permissions(&directory, permissions).unwrap();
    failure(&out, "PKG0005");
    assert_eq!(fs::read(&path).unwrap(), old);
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
}
