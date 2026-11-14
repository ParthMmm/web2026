//! The window, driven in `cargo test` on the CPU with no GPU. Press tests press the real view
//! and check what the person would see. Golden trees build every demo scene, run 1.5 s of
//! frames, and write one line per element to `goldens/<scene>.txt`: indent for depth, then the
//! last part of its path, role, key, label, text, help, rect, fill, text colour and opacity. A
//! change to what any scene shows fails `cargo test`. After a change you meant, rewrite them
//! with `PHOTO_PROTOTYPE_UPDATE_GOLDENS=1 cargo test --features desktop goldens` and read the
//! diff.

use super::{demo, gallery::Gallery, gallery::WINDOW_SIZE, look, on_key, poll, setup, view::view};
use crate::cloud::{CloudFailure, CloudId, CloudState, CloudStep};
use rune::{Env, Key, KeyEvent, Runtime, ScrollBars, text::TextSystem};
use std::{fmt::Write as _, path::PathBuf, sync::Arc};

/// 1.5 s, as `--snap --at 1.5`, in steps long enough to be quick.
const STEPS: usize = 3;
const STEP: f64 = 0.5;

mod cloud;

fn window(apply: fn(&mut Gallery)) -> Runtime<Gallery> {
    let mut gallery = Gallery::demo();
    apply(&mut gallery);
    runtime(gallery)
}

/// The real view, `poll` and keys on `gallery`, settled.
fn runtime(gallery: Gallery) -> Runtime<Gallery> {
    runtime_waking(gallery, Arc::new(|| {}))
}

/// As `runtime`, with `waker` standing in for the host's wake-up.
fn runtime_waking(gallery: Gallery, waker: rune::Waker) -> Runtime<Gallery> {
    let text = TextSystem::new(look::FONTS, look::FAMILIES);
    let mut rt = Runtime::with_text(gallery, view, WINDOW_SIZE, Env::new(true, 1.0), text);
    rt.poll = Some(poll);
    rt.on_key = Some(on_key);
    rt.set_system_scroll_bars(ScrollBars::Always);
    setup(&mut rt.state, waker);
    settle(&mut rt);
    rt
}

fn press(rt: &mut Runtime<Gallery>, key: &str) {
    let (x, y) = rt
        .find(key)
        .unwrap_or_else(|| panic!("no element keyed {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn key(rt: &mut Runtime<Gallery>, key: Key) {
    rt.key(KeyEvent::from(key));
    settle(rt);
}

/// Runs the same 1.5 s as a render, so a sheet that slides in has arrived. One long frame
/// does not move a slide past its first step.
fn settle(rt: &mut Runtime<Gallery>) {
    for _ in 0..STEPS {
        rt.frame(STEP);
    }
    rt.settle_all();
    rt.frame(0.0);
}

/// The text shown under the element keyed `key`, joined in reading order.
fn text_in(rt: &Runtime<Gallery>, key: &str) -> String {
    let elements = rt.elements();
    let Some(root) = elements
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
    else {
        return String::new();
    };
    let prefix = format!("{}/", root.path);
    elements
        .iter()
        .filter(|e| !e.exiting && (e.path == root.path || e.path.starts_with(&prefix)))
        .filter_map(|e| e.text.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

fn shows(rt: &Runtime<Gallery>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn name(rt: &Runtime<Gallery>, index: usize) -> String {
    rt.state.photos[index].record.file_name()
}

#[test]
fn pressing_a_tile_shows_it_in_the_inspector() {
    let mut rt = window(|_| {});
    let tile = rt.state.photos[4].key.clone();
    press(&mut rt, &format!("{tile}/hit"));
    assert!(text_in(&rt, "inspector").contains(&name(&rt, 4)));
}

#[test]
fn keys_step_through_the_viewer() {
    let mut rt = window(|_| {});
    key(&mut rt, Key::Right);
    assert!(
        text_in(&rt, "inspector").contains(&name(&rt, 1)),
        "Right selects the next photo"
    );
    key(&mut rt, Key::Enter);
    assert!(shows(&rt, "viewer"), "Return opens the viewer");
    key(&mut rt, Key::Right);
    let bar = text_in(&rt, "viewer-bar");
    assert!(
        bar.contains(&name(&rt, 2)) && bar.contains("3 of 24"),
        "Right steps the viewer: {bar}"
    );
    key(&mut rt, Key::Escape);
    assert!(!shows(&rt, "viewer"), "Esc closes the viewer");
}

#[test]
fn clearing_the_failures_hides_the_attention_row() {
    let mut rt = window(|s| s.failures = demo::failures());
    press(&mut rt, "not-imported");
    assert!(shows(&rt, "failures-sheet"), "the row opens the list");
    press(&mut rt, "clear-failures");
    assert!(rt.state.failures.is_empty(), "the list is cleared");
    assert!(!shows(&rt, "failures-sheet"), "the list closes");
    assert!(!shows(&rt, "not-imported"), "the sidebar row leaves");
}

#[test]
fn the_title_bar_hides_the_panels_and_zooms_the_grid() {
    let mut rt = window(|_| {});
    let columns = rt.state.layout().columns;
    press(&mut rt, "toggle-sidebar");
    press(&mut rt, "toggle-info");
    assert!(!shows(&rt, "sidebar-pane"), "the sidebar hides");
    assert!(!shows(&rt, "inspector"), "the inspector hides");
    assert!(
        rt.state.layout().columns > columns,
        "the grid takes the room"
    );
    let wide = rt.state.layout().columns;
    rt.state.zoom(2);
    settle(&mut rt);
    assert!(
        rt.state.layout().columns < wide,
        "Zoom In makes tiles larger"
    );
}

fn scene(name: &str) -> fn(&mut Gallery) {
    demo::SCENES
        .iter()
        .find(|(scene, _)| *scene == name)
        .unwrap_or_else(|| panic!("no scene {name}"))
        .1
}

#[test]
fn the_cloud_problems_row_lists_each_reason() {
    let mut rt = window(scene("upload-failed"));
    press(&mut rt, "cloud-problems");
    assert!(shows(&rt, "cloud-failures-sheet"), "the row opens the list");
    let list = text_in(&rt, "cloud-failures-sheet");
    assert!(
        list.contains("didn't answer") && list.contains("w1200 carries metadata"),
        "{list}"
    );
    press(&mut rt, "close-cloud-failures");
    assert!(!shows(&rt, "cloud-failures-sheet"), "the list closes");
}

#[test]
fn the_cloud_problems_list_names_the_step_that_stopped() {
    let mut rt = window(scene("publish-failed"));
    press(&mut rt, "cloud-problems");
    let list = text_in(&rt, "cloud-failures-sheet");
    assert!(
        list.contains("Couldn't publish") && list.contains("Couldn't unpublish"),
        "{list}"
    );
}

#[test]
fn a_draft_offers_publish_and_says_it_is_private() {
    let mut rt = window(scene("description-edit"));
    let cloud = text_in(&rt, "cloud");
    assert!(cloud.contains("Only you can see it"), "{cloud}");
    assert!(shows(&rt, "publish-photo") && !shows(&rt, "unpublish-photo"));
    let before = rt.state.photos[0].record.cloud.clone();
    press(&mut rt, "publish-photo");
    assert_eq!(
        rt.state.photos[0].record.cloud, before,
        "the demo library makes no calls"
    );
}

#[test]
fn a_published_photo_offers_unpublish_and_says_it_is_public() {
    let rt = window(scene("published"));
    let cloud = text_in(&rt, "cloud");
    assert!(cloud.contains("Anyone can see it"), "{cloud}");
    assert!(shows(&rt, "unpublish-photo") && !shows(&rt, "publish-photo"));
}

#[test]
fn a_failed_publish_offers_a_retry_of_that_step() {
    let rt = window(scene("publish-failed"));
    assert!(shows(&rt, "retry-publish"), "{}", text_in(&rt, "cloud"));
    let rt = window(scene("unpublish-failed"));
    assert!(shows(&rt, "retry-unpublish"), "{}", text_in(&rt, "cloud"));
    assert!(text_in(&rt, "cloud").contains("It may still be public"));
}

#[test]
fn the_status_bar_says_what_is_being_published_and_how_it_ended() {
    let mut rt = window(scene("published"));
    let id = rt.state.photos[0].record.id.clone();
    let cloud_id = CloudId::from_hex("ab");
    rt.state.cloud_changed(
        &id,
        CloudState::Publishing {
            cloud_id: cloud_id.clone(),
        },
    );
    settle(&mut rt);
    assert_eq!(text_in(&rt, "publish-status"), "Publishing 1 photo…");
    rt.state
        .cloud_changed(&id, CloudState::Published { cloud_id });
    settle(&mut rt);
    assert!(!shows(&rt, "publish-status"));
    assert_eq!(
        text_in(&rt, "status"),
        format!("Published {}", name(&rt, 0))
    );
}

#[test]
fn a_description_is_counted_and_saved_as_one_line() {
    let mut rt = window(scene("published"));
    press(&mut rt, "description");
    rt.paste("  The tram at dawn\n ");
    settle(&mut rt);
    let length = "The tram at dawn".len()
        + rt.state.photos[1]
            .record
            .description
            .as_ref()
            .map_or(0, |kept| kept.chars().count() + 1);
    assert!(
        text_in(&rt, "description-count").starts_with(&format!("{length} / ")),
        "{}",
        text_in(&rt, "description-count")
    );
    assert!(shows(&rt, "save-description"));
    press(&mut rt, "save-description");
    let kept = rt.state.photos[1].record.description.clone().unwrap();
    assert!(!kept.contains('\n') && !kept.contains("  "), "{kept:?}");
    assert!(kept.contains("The tram at dawn"), "{kept:?}");
    assert!(!shows(&rt, "save-description"), "nothing is left to save");
}

#[test]
fn a_description_over_the_limit_says_so_and_is_not_saved() {
    let mut rt = window(scene("description-long"));
    let before = rt.state.photos[1].record.description.clone();
    assert!(shows(&rt, "description-too-long"));
    press(&mut rt, "save-description");
    assert_eq!(rt.state.photos[1].record.description, before);
}

#[test]
fn upload_in_the_demo_library_starts_nothing() {
    let mut rt = window(|_| {});
    assert!(text_in(&rt, "cloud").contains("Only in this library"));
    press(&mut rt, "upload-photo");
    assert!(
        rt.state
            .photos
            .iter()
            .all(|photo| photo.record.cloud == CloudState::Local),
        "the demo library makes no calls and queues nothing"
    );
}

#[test]
fn the_status_bar_counts_an_upload_run_and_then_sums_it_up() {
    let mut rt = window(|_| {});
    let ids: Vec<_> = rt.state.photos[..3]
        .iter()
        .map(|photo| photo.record.id.clone())
        .collect();
    for id in &ids {
        rt.state.cloud_changed(id, CloudState::Queued);
    }
    settle(&mut rt);
    assert!(
        text_in(&rt, "upload-progress").contains("0 of 3"),
        "{}",
        text_in(&rt, "status-bar")
    );
    let cloud_id = CloudId::from_hex("ab");
    rt.state.cloud_changed(
        &ids[0],
        CloudState::Draft {
            cloud_id: cloud_id.clone(),
        },
    );
    rt.state.cloud_changed(
        &ids[1],
        CloudState::Draft {
            cloud_id: cloud_id.clone(),
        },
    );
    rt.state.cloud_changed(
        &ids[2],
        CloudState::Failed {
            cloud_id: None,
            failure: CloudFailure::OriginalMissing,
            step: CloudStep::Upload,
        },
    );
    settle(&mut rt);
    assert!(
        !shows(&rt, "upload-progress"),
        "the bar leaves when nothing is left"
    );
    assert_eq!(
        text_in(&rt, "status"),
        "Uploaded 2 photos as private drafts · 1 failed"
    );
}

/// The settled window's elements, one line each.
fn tree(apply: fn(&mut Gallery)) -> String {
    let rt = window(apply);
    let mut out = String::new();
    for e in rt.elements().iter().filter(|e| !e.exiting) {
        let depth = e.path.matches('/').count();
        let name = e.path.rsplit('/').next().unwrap_or_default();
        let field = |v: &Option<String>| {
            v.as_deref()
                .map_or_else(|| "-".to_owned(), |t| t.replace('\n', "\\n"))
        };
        let [x, y, w, h] = e.rect.map(|v| (v * 10.0).round() / 10.0);
        let color = e.text_color.map_or_else(|| "-".to_owned(), |c| c.hex());
        let _ = writeln!(
            out,
            "{:depth$}{name}\t{}\t{}\t{}\t{}\t{}\t{x},{y},{w},{h}\t{}\t{color}\t{:.2}",
            "",
            e.role,
            field(&e.key),
            field(&e.label),
            field(&e.text),
            field(&e.help),
            e.fill.hex(),
            e.opacity,
        );
    }
    out
}

/// The checkout this run tests, read at run time, so a test binary built in
/// one worktree and run from another writes only where it was run.
fn checkout() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from)
}

fn first_difference(want: &str, got: &str) -> String {
    let (want, got): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let at = want
        .iter()
        .zip(&got)
        .position(|(a, b)| a != b)
        .unwrap_or(want.len().min(got.len()));
    let line = |v: &[&str]| {
        v.get(at)
            .map_or("(end)", |l| l.trim_start())
            .chars()
            .take(160)
            .collect::<String>()
    };
    format!("line {}: was {} | now {}", at + 1, line(&want), line(&got))
}

#[test]
fn goldens_every_scene_draws_as_its_golden() {
    let update = std::env::var_os("PHOTO_PROTOTYPE_UPDATE_GOLDENS").is_some();
    let folder = checkout().join("goldens");
    let actual = checkout().join("target/goldens-actual");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::create_dir_all(&actual).unwrap();
    let mut differ = Vec::new();
    for (name, apply) in demo::SCENES {
        let got = tree(*apply);
        let path = folder.join(format!("{name}.txt"));
        if update {
            std::fs::write(&path, &got).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        if want != got {
            std::fs::write(actual.join(format!("{name}.txt")), &got).unwrap();
            differ.push(format!("  {name}: {}", first_difference(&want, &got)));
        }
    }
    assert!(
        differ.is_empty(),
        "{} of {} scenes differ from goldens/ (now in target/goldens-actual/). If the change is meant, rewrite them with PHOTO_PROTOTYPE_UPDATE_GOLDENS=1 cargo test --features desktop goldens, and read the diff.\n{}",
        differ.len(),
        demo::SCENES.len(),
        differ.join("\n")
    );
}
