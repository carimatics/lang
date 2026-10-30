//! Read-only historical input facts. Canonical shape is not authentication;
//! execution and lock replay still require the original frozen Graph.
use super::*;
use crate::package::manifest::{valid_name, valid_version};

pub const MAX_FACTS: usize = 65_536;
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Target,
    Compiler,
    Manifest,
    Package,
    Source,
    Dependency,
    Binding,
    Workspace,
    Checksum,
    Sdk,
    Rustc,
}
pub type Facts = BTreeMap<(Category, Vec<String>), String>;
pub(crate) struct Inspection {
    pub target: String,
    pub compiler_build_id: String,
    pub facts: Facts,
}
pub(crate) fn read_record(
    directory: &Dir,
    display: &Path,
    path: &Path,
    maximum: usize,
) -> Result<Vec<u8>, Diagnostic> {
    graph::read(directory, display, path, maximum).map(|input| input.bytes)
}
fn normalized(value: &str, root: bool) -> Result<(), Diagnostic> {
    if value.is_empty()
        || value.len() > 8192
        || value.contains(['\\', ':', '\0'])
        || !(root && value == ".")
            && value
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(error("invalid historical input path"));
    }
    Ok(())
}
fn hash(value: &str) -> Result<(), Diagnostic> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(error("invalid historical input digest"));
    }
    Ok(())
}
fn put(facts: &mut Facts, category: Category, key: &[&str], value: &str) -> Result<(), Diagnostic> {
    if facts.len() == MAX_FACTS
        || value.len() > 8192
        || value.contains('\0')
        || key.len() > 4
        || key.iter().any(|s| s.len() > 8192 || s.contains('\0'))
        || facts
            .insert(
                (category, key.iter().map(|s| (*s).into()).collect()),
                value.into(),
            )
            .is_some()
    {
        return Err(error("duplicate or oversized historical input fact"));
    }
    Ok(())
}
fn canonical(record: &impl Serialize, bytes: &[u8]) -> Result<(), Diagnostic> {
    let mut expected = Bytes(Vec::new());
    serde_json::to_writer_pretty(&mut expected, record).map_err(|_| capacity())?;
    expected.write_all(b"\n").map_err(|_| capacity())?;
    if expected.0 != bytes {
        return Err(error("historical lock must use its canonical encoding"));
    }
    Ok(())
}
pub(crate) fn inspect(bytes: &[u8]) -> Result<Inspection, Diagnostic> {
    validate_encoding(bytes)?;
    #[derive(Deserialize)]
    struct Header {
        schema: u32,
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| error("invalid lock header"))?;
    let (target, version, build_id, workspace, packages) = if header.schema == 1 {
        let r: Record =
            serde_json::from_slice(bytes).map_err(|_| error("invalid historical lock"))?;
        canonical(&r, bytes)?;
        (
            r.target,
            r.compiler_version,
            r.compiler_build_id,
            None,
            r.packages,
        )
    } else {
        let r: WorkspaceRecord = serde_json::from_slice(bytes)
            .map_err(|_| error("invalid historical workspace lock"))?;
        canonical(&r, bytes)?;
        (
            r.target,
            r.compiler_version,
            r.compiler_build_id,
            Some(r.workspace),
            r.packages,
        )
    };
    if target.len() > 256
        || target
            .parse::<lang_codegen_clif::Triple>()
            .ok()
            .is_none_or(|t| t.to_string() != target)
        || version.is_empty()
        || version.len() > 128
        || !version.bytes().all(|b| b.is_ascii_graphic())
        || packages.is_empty()
        || packages.len() > graph::Limits::default().packages
    {
        return Err(error("invalid historical lock identity or package count"));
    }
    hash(&build_id)?;
    let mut facts = Facts::new();
    put(&mut facts, Category::Target, &["target"], &target)?;
    put(&mut facts, Category::Compiler, &["version"], &version)?;
    put(&mut facts, Category::Compiler, &["build_id"], &build_id)?;
    let roots: BTreeSet<_> = packages.iter().map(|p| p.root.as_str()).collect();
    if roots.len() != packages.len() || !packages.windows(2).all(|p| p[0].root < p[1].root) {
        return Err(error("duplicate or unordered historical package roots"));
    }
    let mut edges = 0;
    for p in &packages {
        normalized(&p.root, true)?;
        normalized(&p.manifest_path, false)?;
        hash(&p.manifest_sha256)?;
        hash(&p.checksum)?;
        if !valid_name(&p.name)
            || !valid_version(&p.version)
            || p.edition != "2026"
            || p.sources.len() > 4096
            || p.bindings.len() > 4096
        {
            return Err(error("invalid historical package metadata or count"));
        }
        put(&mut facts, Category::Package, &[&p.root, "name"], &p.name)?;
        put(
            &mut facts,
            Category::Package,
            &[&p.root, "version"],
            &p.version,
        )?;
        put(
            &mut facts,
            Category::Package,
            &[&p.root, "edition"],
            &p.edition,
        )?;
        put(
            &mut facts,
            Category::Manifest,
            &[&p.root, &p.manifest_path],
            &p.manifest_sha256,
        )?;
        put(&mut facts, Category::Checksum, &[&p.root], &p.checksum)?;
        if !p.sources.windows(2).all(|s| s[0].path < s[1].path)
            || !p.dependencies.windows(2).all(|d| d[0].alias < d[1].alias)
        {
            return Err(error("unordered historical sources or dependencies"));
        }
        for source in &p.sources {
            normalized(&source.path, false)?;
            hash(&source.sha256)?;
            if source.path == p.manifest_path {
                return Err(error("source aliases historical manifest"));
            }
            put(
                &mut facts,
                Category::Source,
                &[&p.root, &source.path],
                &source.sha256,
            )?;
        }
        edges += p.dependencies.len();
        if edges > graph::Limits::default().edges {
            return Err(error("historical dependency limit exceeded"));
        }
        for d in &p.dependencies {
            if !valid_name(&d.alias) || !roots.contains(d.root.as_str()) {
                return Err(error("invalid historical dependency edge"));
            }
            put(
                &mut facts,
                Category::Dependency,
                &[&p.root, &d.alias],
                &d.root,
            )?;
        }
        let mut previous = None;
        for b in &p.bindings {
            for value in [&b.scope, &b.path, &b.target] {
                normalized(value, true)?;
            }
            let order = (&b.scope, b.kind.as_str(), &b.path, &b.target);
            if previous.is_some_and(|p| p >= order) {
                return Err(error("unordered historical input bindings"));
            }
            previous = Some(order);
            put(
                &mut facts,
                Category::Binding,
                &[&p.root, &b.scope, b.kind.as_str(), &b.path],
                &b.target,
            )?;
        }
    }
    if let Some(w) = workspace {
        normalized(&w.manifest_path, false)?;
        normalized(&w.selected_root, true)?;
        hash(&w.manifest_sha256)?;
        hash(&w.checksum)?;
        if w.members.is_empty()
            || w.members.len() > 129
            || !roots.contains(w.selected_root.as_str())
            || !w.members.windows(2).all(|m| m[0].path < m[1].path)
        {
            return Err(error(
                "invalid historical workspace membership or selection",
            ));
        }
        put(
            &mut facts,
            Category::Workspace,
            &["manifest", &w.manifest_path],
            &w.manifest_sha256,
        )?;
        put(
            &mut facts,
            Category::Workspace,
            &["selected_root"],
            &w.selected_root,
        )?;
        put(&mut facts, Category::Workspace, &["checksum"], &w.checksum)?;
        let mut member_roots = BTreeSet::new();
        for m in &w.members {
            normalized(&m.path, true)?;
            normalized(&m.root, true)?;
            normalized(&m.manifest_path, false)?;
            hash(&m.manifest_sha256)?;
            if !valid_name(&m.name) || !valid_version(&m.version) || !member_roots.insert(&m.root) {
                return Err(error("invalid historical workspace member"));
            }
            for (field, value) in [
                ("root", &m.root),
                ("manifest_path", &m.manifest_path),
                ("manifest_sha256", &m.manifest_sha256),
                ("name", &m.name),
                ("version", &m.version),
            ] {
                put(
                    &mut facts,
                    Category::Workspace,
                    &["member", &m.path, field],
                    value,
                )?;
            }
        }
        if !member_roots.contains(&w.selected_root) {
            return Err(error("historical selection is not a workspace member"));
        }
    }
    Ok(Inspection {
        target,
        compiler_build_id: build_id,
        facts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historical_facts_come_from_real_canonical_package_inputs() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/packages/local-dependency/Lang.json");
        let target = lang_codegen_clif::Triple::host().to_string();
        let graph = Graph::resolve(&manifest, &target, Default::default()).unwrap();
        let lock = Lock::capture(&graph).unwrap();
        let input = inspect(lock.as_bytes()).unwrap();
        assert_eq!(input.target, target);
        assert_eq!(
            input.compiler_build_id,
            lang_codegen_clif::sdk::build_id(&lang_codegen_clif::Triple::host())
        );
        assert_eq!(
            input
                .facts
                .get(&(Category::Dependency, vec![".".into(), "helper".into()]))
                .unwrap(),
            "deps/helper"
        );
        assert!(
            input
                .facts
                .keys()
                .any(|(kind, key)| *kind == Category::Source && key[0] == "deps/helper")
        );
        let bytes = std::str::from_utf8(lock.as_bytes()).unwrap();
        let bad = bytes.replacen("\"root\": \".\"", "\"root\": \"..\"", 1);
        assert!(inspect(bad.as_bytes()).is_err());
        assert!(inspect(bytes.trim_end().as_bytes()).is_err());
        let bad = bytes.replacen("{", "{\"schema\":1,", 1);
        assert!(inspect(bad.as_bytes()).is_err());
        assert!(inspect(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
}
