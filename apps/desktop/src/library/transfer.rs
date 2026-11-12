//! The transfer lanes: every call to the photo API runs here, apart from the image lanes, so
//! cloud work never holds an encoder or delays a preview. A person's request (upload, publish,
//! unpublish) first writes the state it starts to the catalog; a lane then reads that state
//! and makes the call it names. Each lane retries what may pass with the same backoff and turns
//! the rest into a failure that names the step, so a Retry runs that step again.

use super::{Event, Shared, publish, upload};
use crate::{
    api::{Api, CallError},
    catalog::PhotoId,
    cloud::{CloudFailure, CloudState, CloudStep},
};
use std::{
    sync::mpsc::SyncSender,
    time::{Duration, Instant},
};

/// At most this many calls run at once. An upload streams one object from disk at a time.
pub(super) const TRANSFER_LANES: usize = 2;

/// What a lane says after the server kept answering a call in a way that asked for another.
pub(super) const CHANGING_ANSWER: &str = "the server kept changing its answer";

/// How a call that failed for a reason that may pass is tried again: up to `attempts` times in
/// all, waiting `first_wait`, then twice as long each time, never longer than `longest_wait`.
/// Uploads, publishes and unpublishes share it.
#[derive(Clone, Copy, Debug)]
pub struct UploadRetry {
    pub attempts: u32,
    pub first_wait: Duration,
    pub longest_wait: Duration,
}

impl Default for UploadRetry {
    fn default() -> Self {
        Self {
            attempts: 5,
            first_wait: Duration::from_secs(2),
            longest_wait: Duration::from_secs(60),
        }
    }
}

impl UploadRetry {
    fn wait(&self, attempt: u32) -> Duration {
        self.first_wait
            .saturating_mul(1 << attempt.min(16))
            .min(self.longest_wait)
    }
}

/// A photo with work under way, waiting for a transfer lane.
pub(super) struct Transfer {
    id: PhotoId,
    due: Instant,
    /// Tries that already failed for a reason that may pass.
    failed_tries: u32,
}

impl Transfer {
    pub(super) fn now(id: PhotoId) -> Self {
        Self {
            id,
            due: Instant::now(),
            failed_tries: 0,
        }
    }
}

/// Starts each step a photo can take from where it is: marks it queued, publishing or
/// unpublishing in the catalog, then hands it on. A photo that can't take the step is skipped,
/// so asking twice does the work once.
pub(super) fn ask(shared: &Shared, requests: Vec<(CloudStep, PhotoId)>) -> Vec<Event> {
    let mut events = Vec::new();
    for (step, id) in requests {
        let mut catalog = shared.catalog();
        if !catalog.contains_photo(&id).unwrap_or(false) {
            continue;
        }
        let Ok(state) = catalog.cloud_state(&id) else {
            continue;
        };
        let Some(next) = state.start(step) else {
            continue;
        };
        if catalog.set_cloud_state(&id, &next).is_err() {
            continue;
        }
        drop(catalog);
        let mut queue = shared.queue();
        match step {
            CloudStep::Upload => queue.prepares.push_back(id.clone()),
            CloudStep::Publish | CloudStep::Unpublish => {
                queue.transfers.push(Transfer::now(id.clone()));
            }
        }
        drop(queue);
        shared.wake.notify_all();
        events.push(Event::CloudChanged { id, state: next });
    }
    events
}

/// What a lane does next.
enum Work {
    /// Compare the catalog with the server's list, once, before any transfer.
    Reconcile,
    Transfer(Transfer),
}

/// One transfer lane: reconciles once if asked, then takes the next due photo, until the
/// library closes.
pub(super) fn run_transfers(shared: &Shared, api: &Api, sender: &SyncSender<Event>) {
    while let Some(work) = next_work(shared) {
        match work {
            Work::Reconcile => {
                let events = publish::reconcile(shared, api);
                shared.queue().reconciling = false;
                shared.wake.notify_all();
                for event in events {
                    if sender.send(event).is_err() {
                        return;
                    }
                }
            }
            Work::Transfer(transfer) => transfer_one(shared, api, sender, transfer),
        }
    }
}

/// The next work, waiting until some is due. While a reconcile runs, no transfer starts, so
/// no lane settles a photo between the server's list and the catalog's correction.
fn next_work(shared: &Shared) -> Option<Work> {
    let mut queue = shared.queue();
    loop {
        if queue.stopped {
            return None;
        }
        if std::mem::take(&mut queue.reconcile_due) {
            queue.reconciling = true;
            return Some(Work::Reconcile);
        }
        let now = Instant::now();
        let next = if queue.reconciling {
            None
        } else {
            queue
                .transfers
                .iter()
                .enumerate()
                .min_by_key(|(_, transfer)| transfer.due)
                .map(|(index, transfer)| (index, transfer.due))
        };
        queue = match next {
            Some((index, due)) if due <= now => {
                return Some(Work::Transfer(queue.transfers.swap_remove(index)));
            }
            Some((_, due)) => {
                shared
                    .wake
                    .wait_timeout(queue, due - now)
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .0
            }
            None => shared
                .wake
                .wait(queue)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        };
    }
}

/// Makes the call the photo's state names, then records the server's answer, retries later, or
/// records why the step stopped.
fn transfer_one(shared: &Shared, api: &Api, sender: &SyncSender<Event>, transfer: Transfer) {
    let id = transfer.id.clone();
    let Ok(state) = shared.catalog().cloud_state(&id) else {
        return;
    };
    let Some(cloud_id) = state.cloud_id().cloned() else {
        return;
    };
    let (step, result) = match state {
        CloudState::Uploading { .. } => {
            let Some(result) = upload::send_prepared(shared, api, sender, &id) else {
                return;
            };
            (CloudStep::Upload, result)
        }
        CloudState::Publishing { .. } => match publish::publish(shared, api, &id, &cloud_id) {
            Ok((state, sent)) => {
                if let Some(event) = publish::record_published(shared, &id, state, &sent) {
                    let _ = sender.send(event);
                }
                return;
            }
            Err(error) => (CloudStep::Publish, Err(error)),
        },
        CloudState::Unpublishing { .. } => {
            (CloudStep::Unpublish, publish::unpublish(api, &cloud_id))
        }
        // Settled since it was queued.
        _ => return,
    };
    if shared.queue().stopped {
        // The library closed mid-call. The catalog keeps the state from before the call, and
        // the next open asks the server again.
        return;
    }
    let failure = match result {
        Ok(state) => {
            let _ = sender.send(record(shared, &id, state));
            return;
        }
        Err(CallError::Stop(failure)) => failure,
        Err(CallError::NotFound) => CloudFailure::NotInCloud,
        Err(error @ (CallError::Transient(_) | CallError::File(_) | CallError::AlreadyDraft)) => {
            let message = match error {
                CallError::Transient(message) => message,
                CallError::File(message) => format!("couldn't read a file to send ({message})"),
                _ => CHANGING_ANSWER.into(),
            };
            let failed_tries = transfer.failed_tries + 1;
            if failed_tries < shared.upload_retry.attempts {
                shared.queue().transfers.push(Transfer {
                    due: Instant::now() + shared.upload_retry.wait(transfer.failed_tries),
                    id,
                    failed_tries,
                });
                shared.wake.notify_all();
                return;
            }
            CloudFailure::Unavailable {
                message: format!("{message}, {failed_tries} tries"),
            }
        }
    };
    let state = CloudState::Failed {
        cloud_id: Some(cloud_id),
        failure,
        step,
    };
    let _ = sender.send(record(shared, &id, state));
}

/// Writes the state to the catalog and returns the event that reports it. A lane that
/// outlives its library writes nothing, so it can't overwrite what a newer library recorded.
pub(super) fn record(shared: &Shared, id: &PhotoId, state: CloudState) -> Event {
    if !shared.queue().stopped {
        let _ = shared.catalog().set_cloud_state(id, &state);
    }
    Event::CloudChanged {
        id: id.clone(),
        state,
    }
}
