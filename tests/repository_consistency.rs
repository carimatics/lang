//! Keep planning arithmetic and translation metadata executable, not hand-maintained promises.

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
