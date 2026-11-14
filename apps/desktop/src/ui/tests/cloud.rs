//! Press tests on a library connected to the fake API from `tests/support`: each press runs its
//! call, and the test waits for the answer to land in the window before it reads what the
//! person sees.

#[path = "../../../tests/support/mod.rs"]
mod support;

use super::{CMD, press, press_tile, runtime, settle, shows, text_in};
use crate::{
    Library,
    cloud::{CloudState, CloudStep},
    ui::gallery::{Gallery, Overlay},
};
use rune::{PopupMenuError, Runtime};
use std::time::{Duration, Instant};
use support::{
    FakeApi, Route, TOKEN,
    flow::{self, Setup},
    unavailable,
};

/// How often the test lets the lanes run while it waits for an answer.
const WAIT_STEP: Duration = Duration::from_millis(5);

struct Connected {
    rt: Runtime<Gallery>,
    api: FakeApi,
    cloud_ids: Vec<String>,
    _setup: Setup,
}

/// A window on `count` drafts in the fake API, the first one selected. The first `published`
/// of them are published through the library before the window opens.
fn connected(count: u8, published: usize, attempts: u32) -> Connected {
    let api = FakeApi::start();
    let setup = flow::setup(count);
    let library = flow::connected(&setup.root, &api, TOKEN, flow::quick_retry(attempts));
    let drafts = flow::drafts(&library, &setup);
    for (id, _) in &drafts[..published] {
        library.publish(vec![id.clone()]);
        flow::wait_for(&library, id, flow::is_published);
    }
    let cloud_ids = drafts.into_iter().map(|(_, cloud_id)| cloud_id).collect();
    Connected {
        rt: on(library),
        api,
        cloud_ids,
        _setup: setup,
    }
}

fn on(library: Library) -> Runtime<Gallery> {
    runtime(Gallery::with_library(library))
}

/// Runs frames until the focused photo's state satisfies `done`, or fails with what it shows.
fn until(rt: &mut Runtime<Gallery>, done: impl Fn(&CloudState) -> bool) {
    let deadline = Instant::now() + flow::TIMEOUT;
    while !rt
        .state
        .focused_photo()
        .is_some_and(|photo| done(&photo.record.cloud))
    {
        assert!(
            Instant::now() < deadline,
            "the window shows {:?}: {}",
            rt.state.focused_photo().map(|photo| &photo.record.cloud),
            text_in(rt, "cloud")
        );
        std::thread::sleep(WAIT_STEP);
        rt.frame(0.05);
    }
    settle(rt);
}

/// Runs frames until each photo at `indices` in the window satisfies `done`.
fn until_each(rt: &mut Runtime<Gallery>, indices: &[usize], done: impl Fn(&CloudState) -> bool) {
    let deadline = Instant::now() + flow::TIMEOUT;
    let states = |rt: &Runtime<Gallery>| -> Vec<CloudState> {
        indices
            .iter()
            .map(|&index| rt.state.photos[index].record.cloud.clone())
            .collect()
    };
    while !states(rt).iter().all(&done) {
        assert!(
            Instant::now() < deadline,
            "the window shows {:?}",
            states(rt)
        );
        std::thread::sleep(WAIT_STEP);
        rt.frame(0.05);
    }
    settle(rt);
}

fn in_gallery(api: &FakeApi, cloud_id: &str) -> Option<serde_json::Value> {
    api.gallery()
        .into_iter()
        .find(|(id, _)| id == cloud_id)
        .map(|(_, details)| details)
}

#[test]
fn pressing_publish_puts_the_draft_in_the_public_gallery() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 0, 4);
    press(&mut rt, "publish-photo");
    until(&mut rt, flow::is_published);
    let cloud = text_in(&rt, "cloud");
    assert!(cloud.contains("Anyone can see it"), "{cloud}");
    assert!(shows(&rt, "unpublish-photo") && !shows(&rt, "publish-photo"));
    assert!(text_in(&rt, "status").starts_with("Published "));
    let details = in_gallery(&api, &cloud_ids[0]).expect("the photo is public");
    assert_eq!(details["film"], "Classic Chrome", "{details}");
}

#[test]
fn pressing_unpublish_takes_the_photo_out_and_says_it_is_private() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 1, 4);
    assert!(text_in(&rt, "cloud").contains("Anyone can see it"));
    press(&mut rt, "unpublish-photo");
    until(&mut rt, flow::is_draft);
    assert!(text_in(&rt, "cloud").contains("Only you can see it"));
    assert!(shows(&rt, "publish-photo"));
    assert!(text_in(&rt, "status").contains("private draft again"));
    assert_eq!(in_gallery(&api, &cloud_ids[0]), None);
}

#[test]
fn a_publish_that_stops_offers_a_retry_that_finishes_it() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 0, 2);
    api.fault(Route::Publish, unavailable(), 2);
    press(&mut rt, "publish-photo");
    until(&mut rt, |state| matches!(state, CloudState::Failed { .. }));
    assert_eq!(
        rt.state.focused_photo().unwrap().record.cloud.retry(),
        Some(CloudStep::Publish)
    );
    assert!(text_in(&rt, "cloud").contains("didn't answer"));
    assert!(shows(&rt, "cloud-problems"), "the sidebar lists it");
    press(&mut rt, "retry-publish");
    until(&mut rt, flow::is_published);
    assert!(in_gallery(&api, &cloud_ids[0]).is_some());
    assert!(!shows(&rt, "cloud-problems"));
}

#[test]
fn a_description_saved_on_a_published_photo_becomes_its_public_alt_text() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(1, 1, 4);
    press(&mut rt, "description");
    rt.paste("Low tide at  noon,\nthe harbour wall");
    settle(&mut rt);
    assert_eq!(text_in(&rt, "description-count"), "34 / 300");
    press(&mut rt, "save-description");
    let alt = |api: &FakeApi| in_gallery(api, &cloud_ids[0]).map(|details| details["alt"].clone());
    let deadline = Instant::now() + flow::TIMEOUT;
    while alt(&api) != Some("Low tide at noon, the harbour wall".into()) {
        assert!(
            Instant::now() < deadline,
            "the gallery's alt is {:?}",
            alt(&api)
        );
        std::thread::sleep(WAIT_STEP);
        rt.frame(0.05);
    }
    until(&mut rt, flow::is_published);
    assert!(!shows(&rt, "save-description"), "nothing is left to save");
}

#[test]
fn the_tile_menu_publishes_a_draft_and_offers_no_unpublish() {
    let Connected {
        mut rt,
        cloud_ids,
        api,
        ..
    } = connected(1, 0, 4);
    let key = rt.state.photos[0].key.clone();
    let (x, y) = rt.find(&key).expect("the first tile");
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
    let menu = rt.popup_menu().expect("a context menu");
    assert!(menu.find("publish").is_some() && menu.find("unpublish").is_some());
    assert!(
        matches!(
            rt.resolve_popup(Some("unpublish")),
            Err(PopupMenuError::Disabled(_))
        ),
        "a draft can't be unpublished"
    );
    rt.resolve_popup(Some("publish")).unwrap();
    until(&mut rt, flow::is_published);
    assert!(in_gallery(&api, &cloud_ids[0]).is_some());
}

/// A window on three photos: the first published, the other two only in the library.
fn published_and_two_local() -> Connected {
    let api = FakeApi::start();
    let setup = flow::setup(3);
    let library = flow::connected(&setup.root, &api, TOKEN, flow::quick_retry(4));
    flow::film_photo(&setup.sources[0], 3);
    let published = flow::import(&library, &setup.sources[..1]).remove(0);
    library.upload(vec![published.clone()]);
    flow::wait_for(&library, &published, flow::is_draft);
    library.publish(vec![published.clone()]);
    flow::wait_for(&library, &published, flow::is_published);
    flow::import(&library, &setup.sources[1..]);
    Connected {
        rt: on(library),
        api,
        cloud_ids: Vec::new(),
        _setup: setup,
    }
}

#[test]
fn upload_on_a_selection_sends_the_local_photos_and_skips_the_published_one() {
    // The setup holds the originals the upload reads, so it stays alive to the end.
    let Connected {
        mut rt,
        api,
        _setup,
        ..
    } = published_and_two_local();
    assert!(flow::is_published(&rt.state.photos[0].record.cloud));
    let begins = api.count(Route::Begin);
    let publishes = api.count(Route::Publish);
    press_tile(&mut rt, 1, CMD);
    press_tile(&mut rt, 2, CMD);
    assert_eq!(text_in(&rt, "status"), "3 photos · 3 selected");
    assert_eq!(text_in(&rt, "upload-selected"), "Upload");
    assert!(text_in(&rt, "cloud").contains("2 photos to upload"));
    press(&mut rt, "upload-selected");
    until_each(&mut rt, &[1, 2], flow::is_draft);
    assert_eq!(
        api.count(Route::Begin) - begins,
        2,
        "one upload for each local photo"
    );
    assert_eq!(
        api.count(Route::Publish),
        publishes,
        "no call for the published photo"
    );
    assert!(
        flow::is_published(&rt.state.photos[0].record.cloud),
        "the published photo stays published"
    );
}

#[test]
fn publish_on_a_selection_publishes_both_drafts_at_once() {
    let Connected {
        mut rt,
        api,
        cloud_ids,
        ..
    } = connected(3, 0, 4);
    press_tile(&mut rt, 1, CMD);
    assert!(text_in(&rt, "cloud").contains("2 drafts to publish"));
    press(&mut rt, "publish-selected");
    until_each(&mut rt, &[0, 1], flow::is_published);
    assert_eq!(api.count(Route::Publish), 2);
    assert!(in_gallery(&api, &cloud_ids[0]).is_some() && in_gallery(&api, &cloud_ids[1]).is_some());
    assert!(
        flow::is_draft(&rt.state.photos[2].record.cloud),
        "the unselected draft stays"
    );
}

#[test]
fn a_publish_moves_a_photo_from_the_drafts_count_to_the_published_one() {
    let Connected { mut rt, .. } = connected(3, 0, 4);
    assert_eq!(text_in(&rt, "drafts"), "Drafts 3");
    assert_eq!(text_in(&rt, "published"), "Published 0");
    press(&mut rt, "publish-photo");
    until(&mut rt, flow::is_published);
    assert_eq!(text_in(&rt, "drafts"), "Drafts 2");
    assert_eq!(text_in(&rt, "published"), "Published 1");
    press(&mut rt, "drafts");
    let shown: Vec<_> = rt
        .state
        .shown_photos()
        .map(|photo| photo.key.clone())
        .collect();
    assert_eq!(shown.len(), 2, "the grid shows the drafts alone");
    assert!(
        rt.state
            .shown_photos()
            .all(|photo| flow::is_draft(&photo.record.cloud)),
        "every photo shown is a draft"
    );
    assert!(
        !shows(&rt, &rt.state.photos[0].key),
        "the published photo leaves the grid"
    );
    assert!(shown.iter().all(|key| shows(&rt, key)));
}

#[test]
fn a_publish_in_drafts_takes_the_published_photos_out_of_the_selection() {
    let Connected { mut rt, .. } = connected(3, 0, 4);
    press(&mut rt, "drafts");
    press_tile(&mut rt, 1, CMD);
    rt.state.open_viewer();
    assert_eq!(text_in(&rt, "status"), "3 photos · 2 selected");
    press(&mut rt, "publish-selected");
    until_each(&mut rt, &[0, 1], flow::is_published);
    assert_eq!(rt.state.shown_len(), 1, "Drafts shows the one draft left");
    assert_eq!(
        rt.state.selection.len(),
        1,
        "no hidden photo stays selected"
    );
    assert_eq!(
        rt.state.focus(),
        Some(&rt.state.photos[2].record.id),
        "the draft left takes the focus"
    );
    let menu = rt.state.menu_state();
    assert!(
        !menu.unpublish,
        "Unpublish is off: no selected photo is published"
    );
    assert!(menu.publish && !menu.several);
    assert!(
        rt.state.overlay != Overlay::Viewer,
        "the viewer of a photo that left the grid closes"
    );
}
