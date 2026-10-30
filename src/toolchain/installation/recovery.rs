use super::*;
use cap_std::fs::Dir;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    kind: MarkerKind,
    root_sha256: String,
    generation: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MarkerKind {
    Uninstall,
}
pub(super) struct Pending {
    marker: Option<Marker>,
    trash: Vec<String>,
    stage: bool,
    next: bool,
}
impl Pending {
    pub fn is_pending(&self) -> bool {
        self.marker.is_some() || !self.trash.is_empty() || self.stage || self.next
    }
}
fn names(dir: &Dir, path: &str, limit: usize) -> Result<Vec<(String, bool)>, Error> {
    let mut result = Vec::new();
    for entry in dir
        .read_dir(path)
        .map_err(|e| io("enumerate recovery entries", path, e))?
    {
        if result.len() >= limit {
            return Err(state("recovery entry limit exceeded"));
        }
        let entry = entry.map_err(|e| io("read recovery entry", path, e))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| state("invalid recovery entry name"))?;
        let kind = entry
            .file_type()
            .map_err(|e| io("inspect recovery entry", path, e))?;
        if !kind.is_file() && !kind.is_dir() {
            return Err(state("nonregular recovery entry refused"));
        }
        result.push((name, kind.is_dir()));
    }
    Ok(result)
}
pub(super) fn pending(store: &storage::Store) -> Result<Pending, Error> {
    let mut next = false;
    let mut marker = None;
    for (name, directory) in names(&store.dir, ".", 10)? {
        let expected_dir = matches!(
            name.as_str(),
            "metadata" | "generations" | "staging" | "trash"
        );
        if directory != expected_dir
            || !matches!(
                name.as_str(),
                "lock"
                    | "root.json"
                    | "state.json"
                    | "metadata"
                    | "generations"
                    | "staging"
                    | "trash"
                    | "state.next"
                    | "transaction.json"
            )
        {
            return Err(state("unknown installation root entry"));
        }
        if name == "state.next" {
            next = true;
        }
        if name == "transaction.json" {
            let bytes = storage::read(&store.dir, Path::new(&name), 1024, true)?;
            let value: Marker = serde_json::from_slice(&bytes)
                .map_err(|_| state("invalid uninstall transaction marker"))?;
            if value.schema != 1 || value.root_sha256 != store.state.root_sha256 {
                return Err(state("uninstall marker identity mismatch"));
            }
            hex::<32>(&value.generation)?;
            marker = Some(value);
        }
    }
    let staging = names(&store.dir, "staging", 1)?;
    if staging.iter().any(|(n, d)| n != "install" || !d) {
        return Err(state("unknown staging entry"));
    }
    let trash = names(&store.dir, "trash", 1)?;
    for (name, directory) in &trash {
        if !directory || marker.as_ref().is_none_or(|m| &m.generation != name) {
            return Err(state("trash lacks a matching transaction marker"));
        }
    }
    Ok(Pending {
        marker,
        trash: trash.into_iter().map(|(n, _)| n).collect(),
        stage: !staging.is_empty(),
        next,
    })
}
fn remove_file(store: &storage::Store, name: &str) -> Result<(), Error> {
    store
        .dir
        .remove_file(name)
        .map_err(|e| io("remove recovery file", name, e))
}
fn remove_tree(store: &storage::Store, name: &str) -> Result<(), Error> {
    store
        .dir
        .remove_dir_all(name)
        .map_err(|e| io("remove recovery directory", name, e))
}
// Refuse nonregular descendants before mutation; never traverse a symlink.
fn inspect_tree(dir: &Dir, path: &str) -> Result<(), Error> {
    let root = dir
        .open_dir(path)
        .map_err(|e| io("open recovery tree", path, e))?;
    let mut pending = vec![String::from(".")];
    let mut count = 0;
    while let Some(prefix) = pending.pop() {
        for (name, directory) in names(&root, &prefix, 4096 * 121)? {
            count += 1;
            let relative = if prefix == "." {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if count > 4096 * 121 || relative.len() > 240 {
                return Err(state("recovery tree bounds exceeded"));
            }
            if directory {
                pending.push(relative);
            }
        }
    }
    Ok(())
}
fn move_tree(store: &storage::Store, from: &str, to: &str) -> Result<(), Error> {
    crate::native::publish::rename(&store.path.join(from), &store.path.join(to))
        .map_err(|e| io("move uninstall generation", from, e))
}
fn checkpoint(point: Checkpoint, fail: &mut impl FnMut(Checkpoint) -> bool) -> Result<(), Error> {
    if fail(point) {
        Err(Error {
            failure: Failure::Injected(point),
            floor_advanced: false,
        })
    } else {
        Ok(())
    }
}
pub(super) fn uninstall(
    store: &mut storage::Store,
    next: storage::State,
    generation: &str,
    fail: &mut impl FnMut(Checkpoint) -> bool,
) -> Result<(), Error> {
    let original = format!("generations/{generation}");
    let trash = format!("trash/{generation}");
    inspect_tree(&store.dir, &original)?;
    let marker = Marker {
        schema: 1,
        kind: MarkerKind::Uninstall,
        root_sha256: store.state.root_sha256.clone(),
        generation: generation.into(),
    };
    let bytes = serde_json::to_vec(&marker).map_err(|_| state("cannot encode uninstall marker"))?;
    storage::write_new(&store.dir, Path::new("transaction.json"), &bytes)?;
    let mut moved = false;
    let result = (|| {
        checkpoint(Checkpoint::AfterUninstallMarker, fail)?;
        move_tree(store, &original, &trash)?;
        moved = true;
        checkpoint(Checkpoint::AfterUninstallMove, fail)?;
        store.commit(next)
    })();
    if let Err(error) = result {
        if !moved || move_tree(store, &trash, &original).is_ok() {
            let _ = remove_file(store, "transaction.json");
        }
        return Err(error);
    }
    checkpoint(Checkpoint::AfterUninstallCommit, fail)?;
    remove_tree(store, &trash)?;
    remove_file(store, "transaction.json")
}
pub(super) fn recover(store: &storage::Store) -> Result<(), Error> {
    let pending = pending(store)?;
    let generations = names(&store.dir, "generations", 128)?;
    let metadata = names(&store.dir, "metadata", 128)?;
    let referenced: BTreeSet<_> = store
        .state
        .installed
        .iter()
        .map(|e| e.generation.as_str())
        .collect();
    let receipts: BTreeSet<_> = store
        .state
        .installed
        .iter()
        .map(|e| e.receipt.metadata_sha256.as_str())
        .chain(
            store
                .state
                .channels
                .iter()
                .map(|f| f.receipt.metadata_sha256.as_str()),
        )
        .collect();
    // Build and validate the entire cleanup plan before any deletion/restoration.
    let mut garbage = Vec::new();
    for (generation, directory) in &generations {
        hex::<32>(generation)?;
        if !directory {
            return Err(state("invalid recovery generation"));
        }
        if !referenced.contains(generation.as_str()) {
            garbage.push(format!("generations/{generation}"));
        }
    }
    if pending.stage {
        garbage.push("staging/install".into());
    }
    for name in &garbage {
        inspect_tree(&store.dir, name)?;
    }
    let mut old_metadata = Vec::new();
    for (name, directory) in metadata {
        let hash = name
            .strip_suffix(".json")
            .ok_or_else(|| state("invalid recovery metadata"))?;
        hex::<32>(hash)?;
        if directory {
            return Err(state("invalid recovery metadata type"));
        }
        if !receipts.contains(hash) {
            old_metadata.push(format!("metadata/{name}"));
        }
    }
    if let Some(marker) = &pending.marker {
        let original = format!("generations/{}", marker.generation);
        let trash = format!("trash/{}", marker.generation);
        let exists = generations.iter().any(|(n, _)| *n == marker.generation);
        let trashed = !pending.trash.is_empty();
        let retained = referenced.contains(marker.generation.as_str());
        if (exists && trashed) || (retained && !exists && !trashed) {
            return Err(state("conflicting or missing uninstall generation"));
        }
        if trashed {
            inspect_tree(&store.dir, &trash)?;
        }
        if trashed && retained {
            move_tree(store, &trash, &original)?;
        } else if trashed {
            remove_tree(store, &trash)?;
        }
        remove_file(store, "transaction.json")?;
    }
    if pending.next {
        remove_file(store, "state.next")?;
    }
    for name in garbage {
        remove_tree(store, &name)?;
    }
    for name in old_metadata {
        remove_file(store, &name)?;
    }
    Ok(())
}
