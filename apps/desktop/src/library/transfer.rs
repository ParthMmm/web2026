//! The transfer lanes: every call to the photo API runs here, apart from the image lanes, so
//! cloud work never holds an encoder or delays a preview. A person's request (upload, publish,
//! unpublish) first writes the state it starts to the catalog; a lane then reads that state
//! and makes the call it names. Each lane retries what may pass with the same backoff and turns
//! the rest into a failure that names the step, so a Retry runs that step again.
//!
//! Two things stop work a lane holds. A cancel returns a queued or uploading photo to Local,
//! and the lane's [`Claim`] on it lapses, so nothing the lane learns after is recorded. A refused
//! owner token fails that one photo and pauses every lane until a Retry finds a new token.

use super::{Event, Queue, Shared, publish, upload};
use crate::{
    api::{Api, CallError},
    catalog::PhotoId,
    cloud::{CloudFailure, CloudState, CloudStep},
};
use std::{
    fs,
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

/// What the person asked of a photo's cloud work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloudRequest {
    Start(CloudStep),
    /// Stop a queued or uploading photo's upload.
    Cancel,
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

/// A lane's hold on one photo's work: the photo, and how many times its upload was cancelled
/// when the lane took it. A cancel or a closed library ends the hold, and from then on the
/// lane records nothing for the photo.
pub(super) struct Claim {
    pub(super) id: PhotoId,
    cancels: u64,
}

impl Claim {
    pub(super) fn take(shared: &Shared, id: &PhotoId) -> Self {
        Self::in_queue(&shared.queue(), id.clone())
    }

    fn in_queue(queue: &Queue, id: PhotoId) -> Self {
        let cancels = queue.cancelled.get(&id).copied().unwrap_or(0);
        Self { id, cancels }
    }

    pub(super) fn holds(&self, queue: &Queue) -> bool {
        !queue.stopped && queue.cancelled.get(&self.id).copied().unwrap_or(0) == self.cancels
    }

    /// Whether the lane may make its next call: not after the hold ends, and not while the
    /// queue is paused.
    pub(super) fn may_call(&self, shared: &Shared) -> Result<(), CallError> {
        let queue = shared.queue();
        if !self.holds(&queue) {
            return Err(CallError::Transient("the work was cancelled".into()));
        }
        if queue.paused {
            return Err(CallError::Paused);
        }
        Ok(())
    }
}

/// Cancels a queued or uploading photo: the catalog says Local, the photo leaves the queue, a
/// lane's claim on it lapses, and its staged copies are deleted. Any other photo is skipped.
fn cancel(shared: &Shared, id: PhotoId) -> Option<Event> {
    let mut queue = shared.queue();
    let mut catalog = shared.catalog();
    let pending = catalog
        .cloud_state(&id)
        .is_ok_and(|state| state.is_pending());
    if !pending || catalog.set_cloud_state(&id, &CloudState::Local).is_err() {
        return None;
    }
    drop(catalog);
    *queue.cancelled.entry(id.clone()).or_default() += 1;
    queue.prepares.retain(|queued| *queued != id);
    queue.transfers.retain(|transfer| transfer.id != id);
    drop(queue);
    let _ = fs::remove_dir_all(shared.upload_dir(&id));
    Some(Event::CloudChanged {
        id,
        state: CloudState::Local,
    })
}

/// Takes the person's requests in order. A step starts where the photo can take it: the
/// catalog says queued, publishing or unpublishing, then the photo is handed on. A photo that
/// can't take the step is skipped, so asking twice does the work once. While the queue is
/// paused, a Retry of a photo the API refused first reads the owner token again: a new one
/// resumes the queue, and the same one leaves that photo failed.
pub(super) fn ask(shared: &Shared, mut requests: Vec<(CloudRequest, PhotoId)>) -> Vec<Event> {
    let mut events = Vec::new();
    let refused = |id: &PhotoId| {
        matches!(
            shared.catalog().cloud_state(id),
            Ok(CloudState::Failed {
                failure: CloudFailure::Unauthorized,
                ..
            })
        )
    };
    let paused = shared.queue().paused;
    let retries = |(request, id): &(CloudRequest, PhotoId)| {
        matches!(request, CloudRequest::Start(_)) && refused(id)
    };
    if paused && requests.iter().any(retries) {
        if resume(shared) {
            events.push(Event::CloudResumed);
        } else {
            let (skipped, kept): (Vec<_>, Vec<_>) = requests.into_iter().partition(retries);
            requests = kept;
            // The window showed each skipped photo starting; it still failed.
            events.extend(skipped.into_iter().filter_map(|(_, id)| {
                let state = shared.catalog().cloud_state(&id).ok()?;
                Some(Event::CloudChanged { id, state })
            }));
            events.push(Event::TokenUnchanged);
        }
    }
    for (request, id) in requests {
        let step = match request {
            CloudRequest::Start(step) => step,
            CloudRequest::Cancel => {
                events.extend(cancel(shared, id));
                continue;
            }
        };
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

/// Reads the owner token again. A token that differs from the one in use replaces it and
/// resumes the queue; returns whether it did.
fn resume(shared: &Shared) -> bool {
    let Some(api) = &shared.api else {
        return false;
    };
    let Some(token) = (shared.token_source)() else {
        return false;
    };
    if api.reauthorize(token) != Ok(true) {
        return false;
    }
    shared.queue().paused = false;
    shared.wake.notify_all();
    true
}

/// What a lane does next.
enum Work {
    /// Compare the catalog with the server's list, once, before any transfer.
    Reconcile,
    Transfer(Transfer, Claim),
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
            Work::Transfer(transfer, claim) => transfer_one(shared, api, sender, transfer, &claim),
        }
    }
}

/// The next work, waiting until some is due. While a reconcile runs, no transfer starts, so
/// no lane settles a photo between the server's list and the catalog's correction. While the
/// queue is paused, none starts either.
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
        let next = if queue.reconciling || queue.paused {
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
                let transfer = queue.transfers.swap_remove(index);
                let claim = Claim::in_queue(&queue, transfer.id.clone());
                return Some(Work::Transfer(transfer, claim));
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
/// records why the step stopped. The first refused token pauses the queue; a call that meets
/// the pause goes back to wait, in its state.
fn transfer_one(
    shared: &Shared,
    api: &Api,
    sender: &SyncSender<Event>,
    transfer: Transfer,
    claim: &Claim,
) {
    let id = claim.id.clone();
    let Ok(state) = shared.catalog().cloud_state(&id) else {
        return;
    };
    let Some(cloud_id) = state.cloud_id().cloned() else {
        return;
    };
    let (step, result) = match state {
        CloudState::Uploading { .. } => {
            let Some(result) = upload::send_prepared(shared, api, sender, claim) else {
                return;
            };
            (CloudStep::Upload, result)
        }
        CloudState::Publishing { .. } => match claim
            .may_call(shared)
            .and_then(|()| publish::publish(shared, api, &id, &cloud_id))
        {
            Ok((state, sent)) => {
                if let Some(event) = publish::record_published(shared, claim, state, &sent) {
                    let _ = sender.send(event);
                }
                return;
            }
            Err(error) => (CloudStep::Publish, Err(error)),
        },
        CloudState::Unpublishing { .. } => (
            CloudStep::Unpublish,
            claim
                .may_call(shared)
                .and_then(|()| publish::unpublish(api, &cloud_id)),
        ),
        // Settled since it was queued.
        _ => return,
    };
    let mut queue = shared.queue();
    if !claim.holds(&queue) {
        // The library closed or the person cancelled mid-call. The catalog keeps what it holds
        // now, and the next open or upload asks the server again.
        return;
    }
    let mut pausing = false;
    let failure = match result {
        Ok(state) => {
            drop(queue);
            report(sender, record(shared, claim, state));
            return;
        }
        Err(CallError::Paused | CallError::Superseded) => {
            // The call met the pause, or a refusal of a token a Retry already replaced: wait
            // in its state, and try again with the token in use.
            queue.transfers.push(Transfer {
                due: Instant::now(),
                ..transfer
            });
            shared.wake.notify_all();
            return;
        }
        Err(CallError::Stop(CloudFailure::Unauthorized)) if queue.paused => {
            // Another lane met the refusal first and already said so.
            queue.transfers.push(Transfer {
                due: Instant::now(),
                ..transfer
            });
            return;
        }
        Err(CallError::Stop(failure)) => {
            if failure == CloudFailure::Unauthorized {
                queue.paused = true;
                pausing = true;
            }
            failure
        }
        Err(CallError::NotFound) => CloudFailure::NotInCloud,
        Err(error @ (CallError::Transient(_) | CallError::File(_) | CallError::AlreadyDraft)) => {
            let message = match error {
                CallError::Transient(message) => message,
                CallError::File(message) => format!("couldn't read a file to send ({message})"),
                _ => CHANGING_ANSWER.into(),
            };
            let failed_tries = transfer.failed_tries + 1;
            if failed_tries < shared.upload_retry.attempts {
                queue.transfers.push(Transfer {
                    due: Instant::now() + shared.upload_retry.wait(transfer.failed_tries),
                    id,
                    failed_tries,
                });
                drop(queue);
                shared.wake.notify_all();
                return;
            }
            CloudFailure::Unavailable {
                message: format!("{message}, {failed_tries} tries"),
            }
        }
    };
    drop(queue);
    let state = CloudState::Failed {
        cloud_id: Some(cloud_id),
        failure,
        step,
    };
    report(sender, record(shared, claim, state));
    if pausing {
        let _ = sender.send(Event::CloudPaused);
    }
}

/// Writes the state to the catalog and returns the event that reports it, while the lane's
/// claim holds. A lane whose photo was cancelled, or that outlives its library, writes nothing,
/// so it can't overwrite the cancel or what a newer library recorded.
pub(super) fn record(shared: &Shared, claim: &Claim, state: CloudState) -> Option<Event> {
    let queue = shared.queue();
    if !claim.holds(&queue) {
        return None;
    }
    let _ = shared.catalog().set_cloud_state(&claim.id, &state);
    drop(queue);
    Some(Event::CloudChanged {
        id: claim.id.clone(),
        state,
    })
}

/// Sends the event a [`record`] returned, if any.
pub(super) fn report(sender: &SyncSender<Event>, event: Option<Event>) {
    if let Some(event) = event {
        let _ = sender.send(event);
    }
}
