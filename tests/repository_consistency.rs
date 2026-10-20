//! Keep planning arithmetic and translation metadata executable, not hand-maintained promises.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Eq, PartialEq)]
struct Task {
    done: bool,
    priority: String,
    points: u32,
}

fn points(cell: &str) -> u32 {
    cell.trim()
        .trim_matches('*')
        .strip_suffix(" pt")
        .expect("point cell ends in pt")
        .parse()
        .expect("integer point value")
}

fn percentage(done: u32, total: u32) -> String {
    format!("{:.1}%", f64::from(done) * 100.0 / f64::from(total))
}

fn roadmap(document: &str) -> Vec<Vec<Task>> {
    let mut milestones: Vec<Vec<Task>> = Vec::new();
    for line in document.lines() {
        if let Some(heading) = line.strip_prefix("## Milestone ") {
            let index = heading.chars().next().unwrap().to_digit(10).unwrap() as usize;
            assert_eq!(index, milestones.len(), "ordered milestone headings");
            milestones.push(Vec::new());
        } else if line.starts_with("- [x] ") || line.starts_with("- [ ] ") {
            let done = match &line[3..4] {
                "x" => true,
                " " => false,
                _ => panic!("invalid checkbox: {line}"),
            };
            let priority = line
                .split("**")
                .nth(1)
                .unwrap()
                .split(" — ")
                .next()
                .unwrap();
            assert!(matches!(priority, "P0" | "P1"));
            milestones
                .last_mut()
                .expect("task belongs to a milestone")
                .push(Task {
                    done,
                    priority: priority.to_owned(),
                    points: points(line.rsplit("— ").next().unwrap()),
                });
        }
    }
    assert_eq!(milestones.len(), 8);
    let mut total_done = 0;
    let mut total = 0;
    for (index, tasks) in milestones.iter().enumerate() {
        let done: u32 = tasks
            .iter()
            .filter(|task| task.done)
            .map(|task| task.points)
            .sum();
        let cost: u32 = tasks.iter().map(|task| task.points).sum();
        assert!(cost > 0);
        let row = document
            .lines()
            .find(|line| line.starts_with(&format!("| {index} ")))
            .expect("milestone summary row");
        let cells: Vec<_> = row.split('|').map(str::trim).collect();
        assert_eq!(points(cells[2]), done, "milestone {index} completed");
        assert_eq!(points(cells[3]), cost, "milestone {index} total");
        assert_eq!(cells[4].trim_matches('*'), percentage(done, cost));
        let section = document.split("## Milestone ").nth(index + 1).unwrap();
        assert!(
            section.contains(&format!("{done} / {cost} pt")),
            "milestone {index} progress text"
        );
        total_done += done;
        total += cost;
    }
    let overall = document
        .lines()
        .find(|line| line.starts_with("| **Overall**") || line.starts_with("| **全体**"))
        .expect("overall row");
    let cells: Vec<_> = overall.split('|').map(str::trim).collect();
    assert_eq!(points(cells[2]), total_done);
    assert_eq!(points(cells[3]), total);
    assert_eq!(cells[4].trim_matches('*'), percentage(total_done, total));
    milestones
}

#[test]
fn roadmap_totals_and_mirrored_task_states_agree() {
    let english = roadmap(include_str!("../docs/roadmap.md"));
    let japanese = roadmap(include_str!("../docs/ja/roadmap.md"));
    assert_eq!(
        english, japanese,
        "task order, state, priority and cost must agree"
    );
}

#[derive(Debug, Eq, PartialEq)]
struct Delivery<'a> {
    id: &'a str,
    state: &'a str,
    remaining: u32,
    needs: Vec<&'a str>,
    gates: Vec<u32>,
    evidence: &'a str,
}

fn delivery_board_for<'a>(document: &'a str, prefix: &str, gate_count: u32) -> Vec<Delivery<'a>> {
    let rows: Vec<_> = document
        .lines()
        .filter(|line| line.starts_with(prefix))
        .map(|line| {
            let cells: Vec<_> = line.split('|').map(str::trim).collect();
            assert_eq!(cells.len(), 9, "seven delivery columns: {line}");
            assert!(!cells[6].is_empty(), "observable acceptance outcome");
            Delivery {
                id: cells[1],
                state: cells[2],
                remaining: cells[3].parse().expect("remaining integer points"),
                needs: if cells[4] == "—" {
                    Vec::new()
                } else {
                    cells[4].split(',').collect()
                },
                gates: cells[5]
                    .split(',')
                    .map(|gate| gate.strip_prefix('G').unwrap().parse().unwrap())
                    .collect(),
                evidence: cells[7],
            }
        })
        .collect();
    assert!(!rows.is_empty());
    let by_id: BTreeMap<_, _> = rows.iter().map(|row| (row.id, row)).collect();
    assert_eq!(by_id.len(), rows.len(), "unique delivery IDs");
    for row in &rows {
        assert!(row.gates.iter().all(|gate| (1..=gate_count).contains(gate)));
        assert!(matches!(row.state, "done" | "doing" | "todo"));
        if row.state == "done" {
            assert_eq!(row.remaining, 0, "no remaining effort for done deliveries");
            let commit = row.evidence.trim_matches('`');
            assert!((7..=40).contains(&commit.len()));
            assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        } else {
            assert!((1..=3).contains(&row.remaining), "split large deliveries");
            assert_eq!(
                row.evidence, "—",
                "incomplete delivery has no done evidence"
            );
        }
        for dependency in &row.needs {
            let prerequisite = by_id.get(dependency).expect("known prerequisite ID");
            if matches!(row.state, "done" | "doing") {
                assert_eq!(prerequisite.state, "done", "prerequisites before work");
            }
        }
    }
    let mut reachable = BTreeSet::new();
    loop {
        let before = reachable.len();
        for row in &rows {
            if row.needs.iter().all(|id| reachable.contains(id)) {
                reachable.insert(row.id);
            }
        }
        if reachable.len() == before {
            break;
        }
    }
    assert_eq!(reachable.len(), rows.len(), "acyclic delivery dependencies");
    let done = rows.iter().filter(|row| row.state == "done").count();
    let doing = rows.iter().filter(|row| row.state == "doing").count();
    let todo = rows.len() - done - doing;
    let remaining: u32 = rows.iter().map(|row| row.remaining).sum();
    assert!(doing <= 1, "one work item in progress");
    assert!(document.contains(&format!(
        "**{done} done · {doing} doing · {todo} todo · {remaining} remaining pt**"
    )));
    let next = document
        .lines()
        .find_map(|line| {
            line.strip_prefix("**Next: ")
                .or_else(|| line.strip_prefix("**次: "))
        })
        .expect("visible next delivery")
        .split_whitespace()
        .next()
        .unwrap();
    if done == rows.len() {
        assert_eq!(next, "none", "completed board has no next delivery");
    } else {
        let next = by_id.get(next).expect("next delivery exists");
        assert_ne!(next.state, "done");
        assert!(next.needs.iter().all(|id| by_id[id].state == "done"));
    }
    rows
}

fn delivery_board(document: &str) -> Vec<Delivery<'_>> {
    delivery_board_for(document, "| M2-", 18)
}

#[test]
fn m3_delivery_evidence_estimates_dependencies_and_translations_agree() {
    let english_text = include_str!("../docs/milestone-3.md");
    let japanese_text = include_str!("../docs/ja/milestone-3.md");
    let english = delivery_board_for(english_text, "| M3-", 15);
    assert_eq!(english, delivery_board_for(japanese_text, "| M3-", 15));
    let accounting = remaining_work_accounting(english_text);
    assert_eq!(accounting, remaining_work_accounting(japanese_text));
    assert_eq!(accounting[3], english.iter().map(|row| row.remaining).sum());
    let gates: BTreeSet<_> = english
        .iter()
        .flat_map(|row| row.gates.iter().copied())
        .collect();
    assert_eq!(gates, (1..=15).collect(), "preserve every M3 gate");
}

fn remaining_work_accounting(document: &str) -> [u32; 4] {
    let equations: Vec<_> = document
        .lines()
        .filter_map(|line| line.strip_prefix("**")?.strip_suffix(" pt**"))
        .filter(|line| line.contains(" + ") && line.contains(" − "))
        .collect();
    assert_eq!(
        equations.len(),
        1,
        "one visible remaining-work reconciliation"
    );
    let parts: Vec<_> = equations[0].split_whitespace().collect();
    assert_eq!(parts.len(), 7);
    assert_eq!([parts[1], parts[3], parts[5]], ["+", "−", "="]);
    let values = [parts[0], parts[2], parts[4], parts[6]]
        .map(|part| part.parse::<u32>().expect("integer planning estimate"));
    let [initial, added, completed, remaining] = values;
    assert_eq!(
        initial
            .checked_add(added)
            .and_then(|sum| sum.checked_sub(completed)),
        Some(remaining),
        "separate work delivered from changes to the estimate"
    );
    assert!(document.contains(&format!(
        "| {initial} pt | +{added} pt | −{completed} pt | {remaining} pt |"
    )));
    values
}

#[test]
fn m2_delivery_evidence_estimates_dependencies_and_translations_agree() {
    let english = delivery_board(include_str!("../docs/milestone-2.md"));
    let japanese = delivery_board(include_str!("../docs/ja/milestone-2.md"));
    assert_eq!(
        english, japanese,
        "delivery metadata and evidence must agree"
    );
    let done = english.iter().filter(|row| row.state == "done").count();
    let doing = english.iter().filter(|row| row.state == "doing").count();
    let todo = english.len() - done - doing;
    let remaining: u32 = english.iter().map(|row| row.remaining).sum();
    let accounting = remaining_work_accounting(include_str!("../docs/milestone-2.md"));
    assert_eq!(
        accounting,
        remaining_work_accounting(include_str!("../docs/ja/milestone-2.md")),
        "estimate changes and completed work agree across translations"
    );
    assert_eq!(
        accounting[3], remaining,
        "accounting agrees with open tasks"
    );
    assert!(include_str!("../docs/milestone-2.md").contains(&format!(
        "**35 → {done} (+{})**",
        done.checked_sub(35).expect("preserve completed deliveries")
    )));
    assert!(include_str!("../docs/ja/milestone-2.md").contains(&format!(
        "**35 → {done}件（+{}件）**",
        done.checked_sub(35).unwrap()
    )));
    let added_tasks = english.len().checked_sub(87).expect("preserve task scope");
    assert!(
        include_str!("../docs/milestone-2.md")
            .contains(&format!("**87 → {} (+{added_tasks})**", english.len()))
    );
    assert!(
        include_str!("../docs/ja/milestone-2.md")
            .contains(&format!("**87 → {}件（+{added_tasks}件）**", english.len()))
    );
    assert!(include_str!("../docs/roadmap.md").contains(&format!(
        "**{done} verified deliveries · {doing} in progress · {todo} remaining tasks (provisional {remaining} pt).**"
    )));
    assert!(include_str!("../docs/ja/roadmap.md").contains(&format!(
        "**検証済み{done}件 · 進行中{doing}件 · 残り{todo}件（暫定{remaining} pt）。**"
    )));
}

fn metadata<'a>(document: &'a str, field: &str) -> &'a str {
    let translated = match field {
        "- Status" => "- ステータス",
        "- Created" => "- 作成日",
        "- Last updated" => "- 最終更新日",
        _ => field,
    };
    document
        .lines()
        .find_map(|line| {
            line.strip_prefix(field)
                .or_else(|| line.strip_prefix(translated))
                .and_then(|suffix| {
                    suffix
                        .strip_prefix(':')
                        .or_else(|| suffix.strip_prefix('：'))
                })
                .map(str::trim)
        })
        .expect("RFC metadata field")
}

#[test]
fn rfc_status_and_dates_match_their_translations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in fs::read_dir(root.join("docs/rfcs")).unwrap() {
        let entry = entry.unwrap();
        if entry
            .path()
            .extension()
            .is_none_or(|extension| extension != "md")
        {
            continue;
        }
        let english = fs::read_to_string(entry.path()).unwrap();
        let japanese =
            fs::read_to_string(root.join("docs/ja/rfcs").join(entry.file_name())).unwrap();
        for field in ["- Status", "- Created", "- Last updated"] {
            assert_eq!(
                metadata(&english, field),
                metadata(&japanese, field),
                "{} {field}",
                entry.path().display()
            );
        }
        assert!(matches!(
            metadata(&english, "- Status"),
            "proposed" | "accepted" | "implemented" | "stabilized" | "withdrawn" | "superseded"
        ));
    }
}

#[test]
fn readme_rfc_indexes_are_complete_and_use_current_statuses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let indexes = [
        (include_str!("../README.md"), "docs/rfcs/", false),
        (include_str!("../README.ja.md"), "docs/ja/rfcs/", true),
    ];
    for entry in fs::read_dir(root.join("docs/rfcs")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if !name.ends_with(".md") || name.starts_with("0000-") {
            continue;
        }
        let document = fs::read_to_string(entry.path()).unwrap();
        let status = metadata(&document, "- Status");
        for (index, prefix, japanese) in indexes {
            let link = format!("]({prefix}{name})");
            let rows: Vec<_> = index.lines().filter(|line| line.ends_with(&link)).collect();
            assert_eq!(rows.len(), 1, "one README entry for {prefix}{name}");
            let status_label = if japanese {
                format!("（{status}）{link}")
            } else {
                format!(" ({status}){link}")
            };
            assert!(
                rows[0].ends_with(&status_label),
                "README status must match {prefix}{name}: {status}"
            );
        }
    }
}
