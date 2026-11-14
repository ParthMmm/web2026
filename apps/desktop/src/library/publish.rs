//! Publishing: the calls that put a draft in the public gallery and take it out, the
//! description a visitor reads, and the check on open that brings the catalog in line with the
//! server. The transfer lanes run every call here (`transfer.rs`).

use super::{
    Event, Shared,
    transfer::{CHANGING_ANSWER, Claim, Transfer},
};
use crate::{
    api::{Api, CallError, Details},
    catalog::{Catalog, PhotoId},
    cloud::{CloudFailure, CloudId, CloudState},
};
use std::collections::HashMap;

/// The API refuses longer alt text.
pub const DESCRIPTION_LIMIT: usize = 300;

/// At most this many pages of the server's list are read on open: 10,000 photos. A longer
/// list is compared as far as it was read, and nothing past it is taken as missing.
const RECONCILE_PAGES: usize = 50;

/// Publishes the photo with the details the catalog holds now, and returns the server's answer
/// with the details it was sent, for [`record_published`].
pub(super) fn publish(
    shared: &Shared,
    api: &Api,
    id: &PhotoId,
    cloud_id: &CloudId,
) -> Result<(CloudState, Details), CallError> {
    let sent = details(&shared.catalog(), id);
    let view = api.publish(cloud_id, &sent).map_err(not_in_cloud)?;
    Ok((CloudState::reported(&view), sent))
}

/// Writes a publish's answer, unless the description changed after it was sent. Then the photo
/// stays Publishing and goes back to a lane, so the server ends with the newest words. The
/// check and the write share one catalog lock, so a description saved at any moment is either
/// seen here or finds the photo Published and publishes it again (`describe`).
pub(super) fn record_published(
    shared: &Shared,
    claim: &Claim,
    state: CloudState,
    sent: &Details,
) -> Option<Event> {
    let id = &claim.id;
    let mut queue = shared.queue();
    if !claim.holds(&queue) {
        return None;
    }
    let mut catalog = shared.catalog();
    if details(&catalog, id) != *sent {
        drop(catalog);
        queue.transfers.push(Transfer::now(id.clone()));
        drop(queue);
        shared.wake.notify_all();
        return None;
    }
    let _ = catalog.set_cloud_state(id, &state);
    Some(Event::CloudChanged {
        id: id.clone(),
        state,
    })
}

/// Unpublishes the photo. A server that stops midway answers 503 and keeps the photo
/// Unpublishing; the lane's retry sends this again, which finishes the removal.
pub(super) fn unpublish(api: &Api, cloud_id: &CloudId) -> Result<CloudState, CallError> {
    api.unpublish(cloud_id)
        .map(|view| CloudState::reported(&view))
        .map_err(not_in_cloud)
}

/// A publish or unpublish of a photo the server doesn't know can't pass by itself.
fn not_in_cloud(error: CallError) -> CallError {
    match error {
        CallError::NotFound => CallError::Stop(CloudFailure::NotInCloud),
        other => other,
    }
}

/// What a visitor sees beside the photo: its description as alt text, and the capture time and
/// film simulation the catalog read from EXIF. Nothing else from EXIF leaves the library.
fn details(catalog: &Catalog, id: &PhotoId) -> Details {
    let Ok(Some(photo)) = catalog.photo(id) else {
        return Details::default();
    };
    Details {
        alt: photo.description,
        captured_at: photo
            .metadata
            .captured_at
            .map(|captured| captured.local().to_owned()),
        film: photo
            .metadata
            .film_simulation
            .map(|film| film.as_str().to_owned()),
    }
}

/// The words of `text` on one line, single-spaced: what a description keeps and counts.
pub fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The words the catalog keeps for a description: one line, single spaces, at most
/// [`DESCRIPTION_LIMIT`] characters, and none when empty.
pub fn clean_description(text: &str) -> Option<String> {
    let capped: String = single_line(text).chars().take(DESCRIPTION_LIMIT).collect();
    let capped = capped.trim_end().to_owned();
    (!capped.is_empty()).then_some(capped)
}

/// Keeps the photo's new description. A published photo whose description changed is published
/// again, so the public alt text follows; the catalog says Publishing before the call.
pub(super) fn describe(shared: &Shared, id: &PhotoId, text: &str) -> Vec<Event> {
    let description = clean_description(text);
    let mut catalog = shared.catalog();
    let Ok(Some(before)) = catalog.photo(id) else {
        return Vec::new();
    };
    if before.description == description {
        return Vec::new();
    }
    if catalog.set_description(id, description.as_deref()).is_err() {
        return Vec::new();
    }
    // A photo still Publishing needs nothing more: its lane sees the new words when it records
    // the answer (`record_published`) and sends them again.
    let mut republish = None;
    if let CloudState::Published { cloud_id } = &before.cloud {
        let publishing = CloudState::Publishing {
            cloud_id: cloud_id.clone(),
        };
        if catalog.set_cloud_state(id, &publishing).is_ok() {
            republish = Some(publishing);
        }
    }
    let photo = catalog.photo(id).ok().flatten();
    drop(catalog);
    let mut events: Vec<Event> = photo
        .into_iter()
        .map(|photo| Event::MetadataUpdated { photo })
        .collect();
    if let Some(state) = republish {
        shared.queue().transfers.push(Transfer::now(id.clone()));
        shared.wake.notify_all();
        events.push(Event::CloudChanged {
            id: id.clone(),
            state,
        });
    }
    events
}

/// Reads the server's list once and corrects each local state that disagrees with it, such as
/// a publish from an earlier run whose answer was lost. Runs while no transfer does. A state
/// that becomes busy (the server is still unpublishing) is handed to a lane to finish.
pub(super) fn reconcile(shared: &Shared, api: &Api) -> Vec<Event> {
    let (views, complete) = match api.list(RECONCILE_PAGES) {
        Ok(listed) => listed,
        Err(error) => {
            return vec![Event::CloudChecked {
                corrected: Err(describe_error(error)),
            }];
        }
    };
    let server: HashMap<&str, CloudState> = views
        .iter()
        .map(|view| (view.photo_id(), CloudState::reported(view)))
        .collect();
    let mut events = Vec::new();
    let mut busy = Vec::new();
    {
        let mut catalog = shared.catalog();
        let Ok(states) = catalog.cloud_states() else {
            return Vec::new();
        };
        for (id, state) in states {
            let Some(cloud_id) = state.cloud_id() else {
                continue;
            };
            let found = server.get(cloud_id.as_str());
            if found.is_none() && !complete {
                continue;
            }
            let Some(corrected) = state.reconciled(found) else {
                continue;
            };
            if catalog.set_cloud_state(&id, &corrected).is_err() {
                continue;
            }
            if corrected.is_busy() {
                busy.push(id.clone());
            }
            events.push(Event::CloudChanged {
                id,
                state: corrected,
            });
        }
    }
    let corrected = events.len();
    shared
        .queue()
        .transfers
        .extend(busy.into_iter().map(Transfer::now));
    shared.wake.notify_all();
    events.push(Event::CloudChecked {
        corrected: Ok(corrected),
    });
    events
}

fn describe_error(error: CallError) -> String {
    match error {
        CallError::Transient(message) | CallError::File(message) => message,
        CallError::NotFound => "HTTP 404".into(),
        CallError::AlreadyDraft => CHANGING_ANSWER.into(),
        CallError::Stop(failure) => failure.message(),
        CallError::Paused => "the queue is paused".into(),
        CallError::Superseded => "the owner token changed during the call".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_is_trimmed_capped_and_empty_means_none() {
        assert_eq!(
            clean_description("  low\n tide  at noon \n"),
            Some("low tide at noon".into())
        );
        assert_eq!(clean_description(" \n\t "), None);
        let long = "é".repeat(DESCRIPTION_LIMIT + 20);
        assert_eq!(
            clean_description(&long).unwrap().chars().count(),
            DESCRIPTION_LIMIT
        );
    }
}
