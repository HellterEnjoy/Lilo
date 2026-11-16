//! Repeatable large-vault workloads.
//!
//! These are ignored by default because generating 10,000 notes takes a while.
//! Run them with a release build so the numbers resemble real use:
//!
//! ```text
//! cargo test --release benchmarks -- --ignored --nocapture --test-threads=1
//! ```

use crate::editor_tools;
use crate::links::LinkIndex;
use crate::quick_switcher;
use crate::search::SearchQuery;
use crate::storage::{self, Note};
use crate::tags::TagIndex;
use std::path::Path;
use std::time::{Duration, Instant};

const FOLDERS: usize = 25;
const TOPICS: [&str; 8] = [
    "project", "meeting", "research", "idea", "rust", "design", "travel", "reading",
];

/// Writes `count` interlinked, tagged Markdown notes spread over nested folders.
fn generate_vault(root: &Path, count: usize) {
    for index in 0..count {
        let folder = root.join(format!("Area {}/Topic {}", index % FOLDERS, index % 7));
        std::fs::create_dir_all(&folder).expect("create folder");
        let topic = TOPICS[index % TOPICS.len()];
        let mut note = Note::new_named(&folder, &format!("{topic} note {index}"));
        note.tags = vec![format!("{topic}/{}", index % 13)];
        note.content = format!(
            "# {topic} {index}\n\nSee [[{} note {}]] and [[{} note {}]], plus [[Missing {}]].\n\n\
             #{topic} #area/{}\n\n{}\n\n- [ ] follow up on {topic}\n",
            TOPICS[(index + 1) % TOPICS.len()],
            (index + 1) % count,
            TOPICS[(index + 3) % TOPICS.len()],
            (index * 7 + 3) % count,
            index % 50,
            index % FOLDERS,
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(12),
        );
        storage::save_note(&note).expect("save generated note");
    }
}

fn time<T>(
    results: &mut Vec<(&'static str, Duration)>,
    label: &'static str,
    work: impl FnOnce() -> T,
) -> T {
    let started = Instant::now();
    let value = work();
    results.push((label, started.elapsed()));
    value
}

fn run(count: usize) {
    let temp = tempfile::tempdir().expect("temp vault");
    let root = temp.path();
    let started = Instant::now();
    generate_vault(root, count);
    let generation = started.elapsed();

    let mut results = Vec::new();
    let (mut notes, warnings, _) = time(&mut results, "load vault from disk", || {
        storage::load_notes(root).expect("load")
    });
    assert_eq!(notes.len(), count);
    assert!(warnings.is_empty());
    time(&mut results, "vault snapshot (watcher)", || {
        storage::vault_snapshot(root).expect("snapshot")
    });
    let links = time(&mut results, "build link index", || {
        LinkIndex::build(&notes, root)
    });
    let tags = time(&mut results, "build tag index", || TagIndex::build(&notes));
    assert!(!tags.all_tags().is_empty());

    let query = SearchQuery::parse("tag:project lorem");
    let hits = time(&mut results, "structured search (all notes)", || {
        notes
            .iter()
            .filter(|note| {
                let folder = note
                    .file_path
                    .parent()
                    .and_then(|parent| parent.strip_prefix(root).ok())
                    .unwrap_or(Path::new(""));
                query.matches_note(note, folder, &[])
            })
            .count()
    });
    assert!(hits > 0);

    let switcher = time(&mut results, "quick switcher query", || {
        quick_switcher::entries("proj note 42", &notes, root, &[])
    });
    assert!(!switcher.is_empty());
    time(&mut results, "wiki-link completion ranking", || {
        editor_tools::rank("resea", notes.iter().map(|note| note.title.as_str()), 8)
    });

    let big_note: String = notes
        .iter()
        .take(200)
        .map(|note| note.content.as_str())
        .collect();
    let matches = time(&mut results, "find in a ~200-note-sized text", || {
        editor_tools::find_matches(&big_note, "ipsum", false)
    });
    assert!(!matches.is_empty());

    let note = &mut notes[count / 2];
    note.content.push_str("\nEdited [[project note 1]]");
    let mut links = links;
    let edited = note.clone();
    time(&mut results, "incremental link refresh (1 note)", || {
        links.refresh_note_content(&edited)
    });

    println!("\n=== Lilo large-vault benchmark: {count} notes (generated in {generation:.2?}) ===");
    for (label, elapsed) in &results {
        println!("{label:<36} {:>10.2?}", elapsed);
    }
}

#[test]
#[ignore = "large-vault benchmark; run explicitly with --ignored"]
fn benchmark_1_000_notes() {
    run(1_000);
}

#[test]
#[ignore = "large-vault benchmark; run explicitly with --ignored"]
fn benchmark_10_000_notes() {
    run(10_000);
}
