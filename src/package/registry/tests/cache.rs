use super::*;
use crate::package::{
    lock::Lock,
    registry::cache::{Cache, Captured},
};

struct IsolatedOffline {
    captured: Captured,
    index: crate::registry::store::Index,
    reads: Cell<usize>,
}
impl Source for IsolatedOffline {
    fn resolve(&self, request: Request<'_>) -> Result<ConsumedArchive, Diagnostic> {
        self.reads.set(self.reads.get() + 1);
        let binding = request.binding.clone();
        let version = request.version;
        let archive = self
            .captured
            .archive(request)
            .map_err(|_| refused("offline cache mismatch"))?;
        // Isolated fixture evidence only: no client, environment or credential lookup.
        ConsumedArchive::isolated_unsigned(binding, &self.index, version, archive.as_bytes())
    }
}

fn inputs(graph: &Graph) -> Vec<&ConsumedArchive> {
    graph
        .packages()
        .iter()
        .filter_map(|p| p.registry())
        .collect()
}
fn local_fixture(temp: &Temp) -> cap_std::fs::Dir {
    fs::write(temp.0.join("Lang.json"), APP).unwrap();
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::write(temp.0.join("src/main.lang"), MAIN).unwrap();
    cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap()
}
fn resolve(
    temp: &Temp,
    directory: &cap_std::fs::Dir,
    binding: &Binding,
    source: &dyn Source,
) -> Graph {
    resolve_path(&temp.0, directory, binding, source)
}
fn resolve_path(
    path: &Path,
    directory: &cap_std::fs::Dir,
    binding: &Binding,
    source: &dyn Source,
) -> Graph {
    let context = Context::new(
        BTreeMap::from([("example".into(), binding.clone())]),
        source,
    )
    .unwrap();
    Graph::resolve_with_registries_in(
        directory.try_clone().unwrap(),
        path.to_path_buf(),
        Path::new("Lang.json"),
        "aarch64-apple-darwin",
        Limits::default(),
        None,
        &context,
    )
    .unwrap()
}

fn fixture_binding() -> Binding {
    Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    )
}

fn next_archive() -> AdmittedArchive {
    let metadata = std::str::from_utf8(METADATA)
        .unwrap()
        .replace("\"version\": \"1.0.0\"", "\"version\": \"1.0.1\"");
    let sources = serde_json::from_str(SOURCE_BYTES).unwrap();
    pack(metadata.as_bytes(), &sources)
}
fn competing_archive() -> AdmittedArchive {
    let original = next_archive();
    let dependency = format!(
        "  \"dependencies\": [\n    {{\n      \"alias\": \"spare\",\n      \"registry\": \"missing\",\n      \"name\": \"spare\",\n      \"version\": \"1.0.0\",\n      \"archive_sha256\": \"{}\",\n      \"targets\": [\n        \"x86_64-pc-windows-msvc\"\n      ]\n    }}\n  ],",
        "33".repeat(32)
    );
    let metadata = std::str::from_utf8(original.descriptor_bytes())
        .unwrap()
        .replace("  \"dependencies\": [],", &dependency);
    assert_ne!(metadata.as_bytes(), original.descriptor_bytes());
    pack(
        metadata.as_bytes(),
        &serde_json::from_str(SOURCE_BYTES).unwrap(),
    )
}
fn write_local(path: &Path, next: bool) {
    let app = std::str::from_utf8(APP).unwrap();
    let main = std::str::from_utf8(MAIN).unwrap();
    fs::write(
        path.join("Lang.json"),
        if next {
            app.replace("1.0.0", "1.0.1")
        } else {
            app.into()
        },
    )
    .unwrap();
    fs::write(
        path.join("src/main.lang"),
        if next {
            main.replace("math::answer();", "math::answer() + math::answer();")
        } else {
            main.into()
        },
    )
    .unwrap();
}

fn child(path: &Path, cut: usize, late_error: bool) -> std::process::Output {
    child_command(path, cut, late_error).output().unwrap()
}
fn child_command(path: &Path, cut: usize, late_error: bool) -> std::process::Command {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "package::registry::tests::cache::publication_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("LANG_M5_CACHE_ROOT", path)
        .env("LANG_M5_CACHE_CHILD", "1")
        .env("LANG_M5_CACHE_CUT", cut.to_string())
        .env_remove("LANG_M5_CACHE_LATE_ERROR");
    if late_error {
        command.env("LANG_M5_CACHE_LATE_ERROR", "1");
    }
    command
}

#[test]
fn publication_child() {
    let Ok(path) = std::env::var("LANG_M5_CACHE_ROOT") else {
        return;
    };
    let path = PathBuf::from(path);
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&path, cap_std::ambient_authority()).unwrap();
    write_local(&path, true);
    let binding = fixture_binding();
    let variant = std::env::var("LANG_M5_CACHE_VARIANT").as_deref() == Ok("competing");
    let memory = IsolatedMemory::new(
        binding.clone(),
        if variant {
            competing_archive()
        } else {
            next_archive()
        },
    );
    let graph = resolve_path(&path, &directory, &binding, &memory);
    let lock = Lock::capture(&graph).unwrap();
    let concurrent = std::env::var("LANG_M5_CACHE_CONCURRENT").as_deref() == Ok("1");
    let parent = std::env::var_os("LANG_M5_CACHE_PARENT")
        .map(PathBuf::from)
        .unwrap_or_else(|| path.clone());
    let output = std::env::var_os("LANG_M5_CACHE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| path.clone());
    let parent = cap_std::fs::Dir::open_ambient_dir(parent, cap_std::ambient_authority()).unwrap();
    let output_directory =
        cap_std::fs::Dir::open_ambient_dir(&output, cap_std::ambient_authority()).unwrap();
    if let (Ok(ready), Ok(go)) = (
        std::env::var("LANG_M5_CACHE_READY"),
        std::env::var("LANG_M5_CACHE_GO"),
    ) {
        fs::write(ready, b"ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !Path::new(&go).exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "cache writer barrier timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let cache = match Cache::open(&parent, "cache") {
        Ok(cache) => cache,
        Err(e) if concurrent && e.kind == crate::registry::store::ErrorKind::Busy => {
            println!("CACHE_BUSY");
            return;
        }
        Err(e) => panic!("{e:?}"),
    };
    let result = cache.publish(&lock, &inputs(&graph), &output_directory);
    if concurrent {
        match result {
            Ok(_) => println!("CACHE_PUBLISHED"),
            Err(e) if e.kind == crate::registry::store::ErrorKind::Conflict => {
                assert_eq!(
                    e.outcome,
                    crate::registry::store::PublicationOutcome::NotPublished
                );
                println!("CACHE_CONFLICT");
            }
            Err(e) => panic!("{e:?}"),
        }
        return;
    }
    if std::env::var("LANG_M5_CACHE_LATE_ERROR").as_deref() == Ok("1") {
        assert_eq!(
            result.unwrap_err().outcome,
            crate::registry::store::PublicationOutcome::Unknown
        );
        assert_eq!(fs::read(output.join("Lang.lock")).unwrap(), lock.as_bytes());
        println!("CACHE_UNKNOWN_OUTCOME");
    } else {
        result.unwrap();
    }
}

fn interruption_case(cut: usize, late_error: bool) -> (usize, bool) {
    let temp = Temp::new();
    let directory = local_fixture(&temp);
    let binding = fixture_binding();
    let old = IsolatedMemory::new(binding.clone(), archive());
    let old_graph = resolve(&temp, &directory, &binding, &old);
    let old_lock = Lock::capture(&old_graph).unwrap();
    let cache = Cache::initialize(&directory, "cache").unwrap();
    cache
        .publish(&old_lock, &inputs(&old_graph), &directory)
        .unwrap();
    drop(cache);
    let result = child(&temp.0, cut, late_error);
    assert_eq!(
        result.status.code(),
        Some(if cut == 0 { 0 } else { 91 }),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let steps = output
        .lines()
        .filter_map(|l| l.split_once("CACHE_CHECKPOINT ").map(|(_, tail)| tail))
        .filter_map(|l| l.split_whitespace().next()?.parse::<usize>().ok())
        .max()
        .unwrap();
    let next = IsolatedMemory::new(binding.clone(), next_archive());
    let next_graph = resolve(&temp, &directory, &binding, &next);
    let next_lock = Lock::capture(&next_graph).unwrap();
    let published = fs::read(temp.0.join("Lang.lock")).unwrap();
    assert!(published == old_lock.as_bytes() || published == next_lock.as_bytes());
    let is_next = published == next_lock.as_bytes();
    if late_error {
        assert!(is_next && output.contains("CACHE_UNKNOWN_OUTCOME"));
        assert!(child(&temp.0, 0, false).status.success());
        assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), published);
    }
    let cache = Cache::open(&directory, "cache").unwrap();
    assert_eq!(
        fs::read_dir(temp.0.join("cache/staging")).unwrap().count(),
        0
    );
    drop(cache);
    write_local(&temp.0, is_next);
    let captured = Captured::open(&directory, "cache", &published).unwrap();
    let offline = IsolatedOffline {
        captured,
        index: if is_next { next.index } else { old.index },
        reads: Cell::new(0),
    };
    let graph = resolve(&temp, &directory, &binding, &offline);
    Lock::capture(&graph).unwrap().verify(&published).unwrap();
    execute(&graph, if is_next { 84 } else { 42 });
    (steps, is_next)
}

#[test]
fn every_real_process_cut_reopens_a_complete_old_or_new_offline_generation() {
    let (steps, complete) = interruption_case(0, false);
    assert!(complete && steps >= 20);
    let mut old = 0;
    let mut new = 0;
    for cut in 1..=steps {
        let (visited, next) = interruption_case(cut, false);
        assert_eq!(visited, cut);
        if next { new += 1 } else { old += 1 }
    }
    assert!(old > 0 && new > 0);
    println!("CACHE_PROCESS_CUTS {steps}; old {old}; new {new}");
}

#[test]
fn unknown_root_outcome_reconciles_the_same_complete_generation() {
    assert!(interruption_case(0, true).1);
}

#[test]
fn bootstrap_child() {
    let Ok(path) = std::env::var("LANG_M5_CACHE_BOOTSTRAP_ROOT") else {
        return;
    };
    let parent = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority()).unwrap();
    drop(Cache::initialize(&parent, "cache").unwrap());
}

fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(path.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let relative = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                result.insert(relative.clone(), Vec::new());
                pending.push(relative);
            } else {
                result.insert(relative.clone(), fs::read(path.join(relative)).unwrap());
            }
        }
    }
    result
}

#[test]
fn interrupted_initialization_never_guesses_or_resets_a_private_root() {
    let mut complete = 0;
    let mut refused = 0;
    for cut in 1..=12 {
        let temp = Temp::new();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "package::registry::tests::cache::bootstrap_child",
                "--nocapture",
            ])
            .env("LANG_M5_CACHE_BOOTSTRAP_ROOT", &temp.0)
            .env("LANG_M5_CACHE_CHILD", "1")
            .env("LANG_M5_CACHE_CUT", cut.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(91),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let before = snapshot(&temp.0);
        match Cache::open(&parent, "cache") {
            Ok(cache) => {
                complete += 1;
                drop(cache);
            }
            Err(_) => {
                refused += 1;
            }
        }
        assert_eq!(snapshot(&temp.0), before);
        assert!(Cache::initialize(&parent, "cache").is_err());
        assert_eq!(snapshot(&temp.0), before);
    }
    assert_eq!((refused, complete), (10, 2));
}

#[test]
fn hostile_cache_inventory_refuses_before_recognized_staging_cleanup() {
    for case in [
        "unknown",
        "profile",
        "hardlink",
        "oversized-stage",
        "missing-object",
        "bad-generation",
    ] {
        let temp = Temp::new();
        let parent = local_fixture(&temp);
        let binding = fixture_binding();
        let memory = IsolatedMemory::new(binding.clone(), archive());
        let graph = resolve(&temp, &parent, &binding, &memory);
        let lock = Lock::capture(&graph).unwrap();
        let cache = Cache::initialize(&parent, "cache").unwrap();
        let generation = cache.publish(&lock, &inputs(&graph), &parent).unwrap();
        drop(cache);
        let stage = temp
            .0
            .join("cache/staging")
            .join(format!("generation-{}", "00".repeat(32)));
        fs::write(&stage, b"partial").unwrap();
        let object = temp.0.join("cache/objects").join(memory.archive.sha256());
        match case {
            "unknown" => fs::write(temp.0.join("cache/foreign"), b"do not delete").unwrap(),
            "profile" => {
                let path = temp.0.join("cache/FORMAT");
                let bytes = fs::read_to_string(&path)
                    .unwrap()
                    .replace("\"schema\": 1", "\"schema\": 2");
                fs::write(path, bytes).unwrap();
            }
            "hardlink" => fs::hard_link(&object, temp.0.join("other-link")).unwrap(),
            "oversized-stage" => {
                let path = temp
                    .0
                    .join("cache/staging")
                    .join(format!("object-{}", "11".repeat(32)));
                fs::File::create(path)
                    .unwrap()
                    .set_len(crate::registry::archive::MAX_ARCHIVE_BYTES as u64 + 1)
                    .unwrap();
            }
            "missing-object" => fs::remove_file(&object).unwrap(),
            "bad-generation" => {
                fs::write(temp.0.join("cache/generations").join(generation), b"{}\n").unwrap()
            }
            _ => unreachable!(),
        }
        assert!(Cache::open(&parent, "cache").is_err(), "{case}");
        assert!(
            Captured::open(&parent, "cache", lock.as_bytes()).is_err(),
            "{case}"
        );
        assert_eq!(fs::read(&stage).unwrap(), b"partial", "{case}");
        assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), lock.as_bytes());
    }
}

#[test]
fn readonly_capture_preserves_staging_while_mutation_recovers_only_verified_orphans() {
    let temp = Temp::new();
    let parent = local_fixture(&temp);
    let binding = fixture_binding();
    let memory = IsolatedMemory::new(binding.clone(), archive());
    let graph = resolve(&temp, &parent, &binding, &memory);
    let lock = Lock::capture(&graph).unwrap();
    let cache = Cache::initialize(&parent, "cache").unwrap();
    cache.publish(&lock, &inputs(&graph), &parent).unwrap();
    assert_eq!(
        Captured::open(&parent, "cache", lock.as_bytes())
            .err()
            .unwrap()
            .kind,
        crate::registry::store::ErrorKind::Busy
    );
    drop(cache);
    let stage = temp
        .0
        .join("cache/staging")
        .join(format!("generation-{}", "00".repeat(32)));
    fs::write(&stage, b"partial").unwrap();
    let unused = next_archive();
    let orphan = temp.0.join("cache/objects").join(unused.sha256());
    fs::write(&orphan, unused.as_bytes()).unwrap();
    let captured = Captured::open(&parent, "cache", lock.as_bytes()).unwrap();
    assert_eq!(fs::read(&stage).unwrap(), b"partial");
    assert_eq!(fs::read(&orphan).unwrap(), unused.as_bytes());
    drop(Cache::open(&parent, "cache").unwrap());
    assert!(!stage.exists() && !orphan.exists());
    assert_eq!(captured.lock_bytes(), lock.as_bytes());
    assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), lock.as_bytes());
}

#[test]
fn local_changes_targets_and_independent_root_policies_cannot_change_locked_replay() {
    let temp = Temp::new();
    let parent = local_fixture(&temp);
    let binding = fixture_binding();
    let memory = IsolatedMemory::new(binding.clone(), archive());
    let graph = resolve(&temp, &parent, &binding, &memory);
    let lock = Lock::capture(&graph).unwrap();
    let cache = Cache::initialize(&parent, "cache").unwrap();
    cache.publish(&lock, &inputs(&graph), &parent).unwrap();
    let different = Binding::new(binding.identity().clone(), [0x33; 32]);
    let second = IsolatedMemory::new(different.clone(), archive());
    let graph = resolve(&temp, &parent, &different, &second);
    let second_lock = Lock::capture(&graph).unwrap();
    assert_ne!(lock.as_bytes(), second_lock.as_bytes());
    cache
        .publish(&second_lock, &inputs(&graph), &parent)
        .unwrap();
    assert_eq!(
        fs::read_dir(temp.0.join("cache/objects")).unwrap().count(),
        1
    );
    assert_eq!(
        fs::read_dir(temp.0.join("cache/generations"))
            .unwrap()
            .count(),
        2
    );
    drop(cache);
    let captured = Captured::open(&parent, "cache", lock.as_bytes()).unwrap();
    assert!(
        captured
            .archive(Request {
                binding: &different,
                name: "math",
                version: "1.0.0",
                archive_sha256: None
            })
            .is_err()
    );
    let offline = IsolatedOffline {
        captured,
        index: memory.index,
        reads: Cell::new(0),
    };
    let graph = resolve(&temp, &parent, &binding, &offline);
    Lock::capture(&graph)
        .unwrap()
        .verify(lock.as_bytes())
        .unwrap();
    execute(&graph, 42);
    let changed_target = String::from_utf8(lock.as_bytes().to_vec())
        .unwrap()
        .replace("aarch64-apple-darwin", "x86_64-pc-windows-msvc");
    assert!(Captured::open(&parent, "cache", changed_target.as_bytes()).is_err());
    fs::write(
        temp.0.join("src/main.lang"),
        "mod math; fn main() -> i64 { return math::answer() + math::answer(); }\n",
    )
    .unwrap();
    let graph = resolve(&temp, &parent, &binding, &offline);
    assert!(
        Lock::capture(&graph)
            .unwrap()
            .verify(lock.as_bytes())
            .is_err()
    );
    assert_eq!(
        fs::read(temp.0.join("Lang.lock")).unwrap(),
        second_lock.as_bytes()
    );
}

#[cfg(unix)]
#[test]
fn cache_symlink_files_and_directories_never_gain_read_or_cleanup_authority() {
    use std::os::unix::fs::symlink;
    for directory_alias in [false, true] {
        let temp = Temp::new();
        let parent = local_fixture(&temp);
        let binding = fixture_binding();
        let memory = IsolatedMemory::new(binding.clone(), archive());
        let graph = resolve(&temp, &parent, &binding, &memory);
        let lock = Lock::capture(&graph).unwrap();
        let cache = Cache::initialize(&parent, "cache").unwrap();
        cache.publish(&lock, &inputs(&graph), &parent).unwrap();
        drop(cache);
        let original = if directory_alias {
            temp.0.join("cache/objects")
        } else {
            temp.0.join("cache/objects").join(memory.archive.sha256())
        };
        let outside = temp.0.join("outside");
        fs::rename(&original, &outside).unwrap();
        symlink(&outside, &original).unwrap();
        assert!(Cache::open(&parent, "cache").is_err());
        assert!(Captured::open(&parent, "cache", lock.as_bytes()).is_err());
        let retained = if directory_alias {
            outside.join(memory.archive.sha256())
        } else {
            outside
        };
        assert_eq!(fs::read(retained).unwrap(), memory.archive.as_bytes());
        assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), lock.as_bytes());
    }
}

#[test]
fn competing_processes_preserve_one_immutable_version_and_exact_offline_identity() {
    use std::process::Stdio;
    for reverse in [false, true] {
        let temp = Temp::new();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
        drop(Cache::initialize(&parent, "cache").unwrap());
        fs::create_dir(temp.0.join("output")).unwrap();
        let go = temp.0.join("go");
        let mut children = Vec::new();
        for i in 0..2 {
            let path = temp.0.join(format!("app{i}"));
            fs::create_dir_all(path.join("src")).unwrap();
            write_local(&path, true);
            let mut command = child_command(&path, 0, false);
            command
                .env("LANG_M5_CACHE_PARENT", &temp.0)
                .env("LANG_M5_CACHE_OUTPUT", temp.0.join("output"))
                .env("LANG_M5_CACHE_CONCURRENT", "1")
                .env("LANG_M5_CACHE_READY", temp.0.join(format!("ready{i}")))
                .env("LANG_M5_CACHE_GO", &go)
                .env(
                    "LANG_M5_CACHE_VARIANT",
                    if (i == 1) ^ reverse {
                        "competing"
                    } else {
                        "original"
                    },
                )
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            children.push(command.spawn().unwrap());
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !(0..2).all(|i| temp.0.join(format!("ready{i}")).exists()) {
            assert!(
                std::time::Instant::now() < deadline,
                "cache writers did not reach barrier"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        fs::write(&go, b"go").unwrap();
        let outputs: Vec<_> = children
            .into_iter()
            .map(|c| c.wait_with_output().unwrap())
            .collect();
        for output in &outputs {
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let logs: Vec<_> = outputs
            .iter()
            .map(|o| String::from_utf8_lossy(&o.stdout))
            .collect();
        let winners: Vec<_> = logs
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("CACHE_PUBLISHED"))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(winners.len(), 1, "{logs:?}");
        let winner = winners[0];
        let loser = 1 - winner;
        assert!(logs[loser].contains("CACHE_BUSY") || logs[loser].contains("CACHE_CONFLICT"));
        let published = fs::read(temp.0.join("output/Lang.lock")).unwrap();
        let mut retry = child_command(&temp.0.join(format!("app{loser}")), 0, false);
        retry
            .env("LANG_M5_CACHE_PARENT", &temp.0)
            .env("LANG_M5_CACHE_OUTPUT", temp.0.join("output"))
            .env("LANG_M5_CACHE_CONCURRENT", "1")
            .env(
                "LANG_M5_CACHE_VARIANT",
                if (loser == 1) ^ reverse {
                    "competing"
                } else {
                    "original"
                },
            );
        let retry = retry.output().unwrap();
        assert!(
            retry.status.success(),
            "{}",
            String::from_utf8_lossy(&retry.stderr)
        );
        assert!(String::from_utf8_lossy(&retry.stdout).contains("CACHE_CONFLICT"));
        assert_eq!(
            fs::read(temp.0.join("output/Lang.lock")).unwrap(),
            published
        );
        assert_eq!(
            fs::read_dir(temp.0.join("cache/objects")).unwrap().count(),
            1
        );
        assert_eq!(
            fs::read_dir(temp.0.join("cache/generations"))
                .unwrap()
                .count(),
            1
        );
        let selected = if (winner == 1) ^ reverse {
            competing_archive()
        } else {
            next_archive()
        };
        let binding = fixture_binding();
        let memory = IsolatedMemory::new(binding.clone(), selected);
        let captured = Captured::open(&parent, "cache", &published).unwrap();
        let offline = IsolatedOffline {
            captured,
            index: memory.index,
            reads: Cell::new(0),
        };
        let app = temp.0.join(format!("app{winner}"));
        let source =
            cap_std::fs::Dir::open_ambient_dir(&app, cap_std::ambient_authority()).unwrap();
        let graph = resolve_path(&app, &source, &binding, &offline);
        Lock::capture(&graph).unwrap().verify(&published).unwrap();
        execute(&graph, 84);
    }
}

#[test]
fn https_publication_reopens_offline_and_freezes_original_vm_native_inputs() {
    let temp = Temp::new();
    let directory = local_fixture(&temp);
    let archive = archive();
    let (mut server, provider, binding) = serve(&directory, &archive);
    let graph = resolve(&temp, &directory, &binding, &provider);
    let lock = Lock::capture(&graph).unwrap();
    let index = provider.client.index("math").unwrap();
    let cache = Cache::initialize(&directory, "cache").unwrap();
    let generation = cache.publish(&lock, &inputs(&graph), &directory).unwrap();
    assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), lock.as_bytes());
    assert_eq!(
        fs::read(temp.0.join("cache/generations").join(generation)).unwrap(),
        lock.as_bytes()
    );
    assert_eq!(
        fs::read(temp.0.join("cache/objects").join(archive.sha256())).unwrap(),
        archive.as_bytes()
    );
    assert_eq!(provider.reads.get(), 1);
    drop(cache);
    server.stop().unwrap();
    drop(server);
    drop(provider);
    drop(graph);

    let captured = Captured::open(&directory, "cache", lock.as_bytes()).unwrap();
    let offline = IsolatedOffline {
        captured,
        index,
        reads: Cell::new(0),
    };
    let graph = resolve(&temp, &directory, &binding, &offline);
    Lock::capture(&graph)
        .unwrap()
        .verify(offline.captured.lock_bytes())
        .unwrap();
    execute(&graph, 42);
    assert_eq!(offline.reads.get(), 1);
    // Neither compiled execution nor later graph analysis consults cache paths.
    fs::remove_dir_all(temp.0.join("cache")).unwrap();
    fs::remove_dir_all(temp.0.join("src")).unwrap();
    fs::remove_file(temp.0.join("Lang.json")).unwrap();
    execute(&graph, 42);
    Lock::capture(&graph)
        .unwrap()
        .verify(lock.as_bytes())
        .unwrap();
    assert_eq!(offline.reads.get(), 1);
}

#[test]
fn exact_reuse_and_current_cache_bytes_are_rechecked_without_replacing_old_lock() {
    let temp = Temp::new();
    let directory = local_fixture(&temp);
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    let memory = IsolatedMemory::new(binding.clone(), archive());
    let graph = resolve(&temp, &directory, &binding, &memory);
    let lock = Lock::capture(&graph).unwrap();
    let cache = Cache::initialize(&directory, "cache").unwrap();
    let generation = cache.publish(&lock, &inputs(&graph), &directory).unwrap();
    assert_eq!(
        cache.publish(&lock, &inputs(&graph), &directory).unwrap(),
        generation
    );
    assert!(cache.publish(&lock, &[], &directory).is_err());
    let internal = directory.open_dir("cache").unwrap();
    assert_eq!(
        cache
            .publish(&lock, &inputs(&graph), &internal)
            .unwrap_err()
            .kind,
        crate::registry::store::ErrorKind::Conflict
    );
    drop(cache);
    let captured = Captured::open(&directory, "cache", lock.as_bytes()).unwrap();
    let wrong = Binding::new(binding.identity().clone(), [0x33; 32]);
    assert!(
        captured
            .archive(Request {
                binding: &wrong,
                name: "math",
                version: "1.0.0",
                archive_sha256: None
            })
            .is_err()
    );
    assert!(
        captured
            .archive(Request {
                binding: &binding,
                name: "math",
                version: "1.0.0",
                archive_sha256: Some(&"00".repeat(32))
            })
            .is_err()
    );
    let object = temp.0.join("cache/objects").join(memory.archive.sha256());
    let mut corrupt = fs::read(&object).unwrap();
    corrupt[0] ^= 1;
    fs::write(&object, &corrupt).unwrap();
    assert!(Captured::open(&directory, "cache", lock.as_bytes()).is_err());
    assert!(Cache::open(&directory, "cache").is_err());
    assert_eq!(fs::read(&object).unwrap(), corrupt);
    assert_eq!(fs::read(temp.0.join("Lang.lock")).unwrap(), lock.as_bytes());
    assert_eq!(
        captured
            .archive(Request {
                binding: &binding,
                name: "math",
                version: "1.0.0",
                archive_sha256: None
            })
            .unwrap()
            .as_bytes(),
        memory.archive.as_bytes()
    );
}
