//! A photo's place in the cloud: the inspector's section with Upload and Retry, and the mark a
//! grid tile carries. Each state has its own icon and words, so no state is told by colour
//! alone.

use super::super::{
    format,
    gallery::{Gallery, GalleryPhoto},
    icons,
    look::Look,
};
use crate::{api::Connection, cloud::CloudState, cloud::ObjectKind};
use rune::{Color, El, Icon, col, icon, row, text};
use rune_kit::{Button, ControlSize, Progress};

const MARK_SIZE: f32 = 20.0;
const MARK_INSET: f32 = 6.0;

fn state_icon(state: &CloudState) -> &'static Icon {
    match state {
        CloudState::Local => &icons::CLOUD_OFF,
        CloudState::Queued => &icons::CLOCK,
        CloudState::Uploading { .. } => &icons::CLOUD_UPLOAD,
        CloudState::Draft { .. } => &icons::CLOUD_CHECK,
        CloudState::Publishing { .. }
        | CloudState::Published { .. }
        | CloudState::Unpublishing { .. } => &icons::GLOBE,
        CloudState::Failed { .. } => &icons::ALERT,
    }
}

fn state_color(state: &CloudState, look: &Look) -> Color {
    if matches!(state, CloudState::Failed { .. }) {
        look.warning
    } else {
        look.muted
    }
}

/// The mark in a tile's top-right corner. A photo only in the library carries none.
pub(super) fn tile_mark(photo: &GalleryPhoto, look: &Look, tile_width: f32) -> Option<El> {
    let state = &photo.record.cloud;
    if *state == CloudState::Local {
        return None;
    }
    let color = if matches!(state, CloudState::Failed { .. }) {
        look.warning
    } else {
        Color::WHITE.mix(Color::BLACK, 0.07)
    };
    Some(
        row()
            .key("cloud-mark")
            .role("img")
            .label(format::cloud_state(state))
            .absolute(tile_width - MARK_SIZE - MARK_INSET, MARK_INSET)
            .size(MARK_SIZE, MARK_SIZE)
            .center()
            .radius(5.0)
            .fill(Color::BLACK.mix(Color::WHITE, 0.04).alpha(0.62))
            .inert()
            .child(icon(state_icon(state), 12.0).color(color)),
    )
}

/// The selected photo's cloud state, what to do about it, and why it can't happen yet.
pub(super) fn cloud_section(s: &Gallery, photo: &GalleryPhoto, look: &Look) -> El {
    let state = &photo.record.cloud;
    let connected = s.is_connected();
    let action = match state {
        CloudState::Local => Some(("Upload", "upload-photo", "Upload as a private draft (⌘U)")),
        CloudState::Failed { failure, .. } if failure.retry_helps() => {
            Some(("Retry", "retry-upload", "Try the upload again (⌘U)"))
        }
        _ => None,
    };
    let heading = row()
        .key("cloud-state")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(icon(state_icon(state), 14.0).color(state_color(state, look)))
        .child(
            text(format::cloud_state(state))
                .font(look.ty.ui_medium)
                .color(look.fg)
                .truncate()
                .grow(1.0),
        )
        .children(action.map(|(label, key, help)| {
            Button::new(label)
                .icon(&icons::CLOUD_UPLOAD)
                .control_size(ControlSize::Small)
                .disabled(!connected)
                .help(help)
                .on_press(|s: &mut Gallery| s.upload_selected())
                .build(look)
                .key(key)
        }));
    let progress = match state {
        CloudState::Uploading { missing, .. } => {
            #[allow(clippy::cast_precision_loss)]
            let value =
                (ObjectKind::ALL.len() - missing.len()) as f32 / ObjectKind::ALL.len() as f32;
            Some(
                Progress::new(Some(value))
                    .build(&progress_kit(look))
                    .key("photo-upload-progress"),
            )
        }
        CloudState::Queued => Some(
            Progress::new(None)
                .build(&progress_kit(look))
                .key("photo-upload-progress"),
        ),
        _ => None,
    };
    let failure = match state {
        CloudState::Failed { failure, .. } => Some(
            text(failure.message())
                .key("upload-failure")
                .role("alert")
                .font(look.ty.small)
                .color(look.muted)
                .wrap(),
        ),
        _ => None,
    };
    let waiting = state.is_pending() || state.can_upload();
    let not_connected = match &s.connection {
        Connection::NotConnected { reason } if waiting => Some(
            text(format!("Not connected to the photo API. {reason}."))
                .key("not-connected")
                .font(look.ty.small)
                .color(look.faint)
                .wrap(),
        ),
        _ => None,
    };
    col()
        .key("cloud")
        .w_full()
        .gap(6.0)
        .padding(10.0)
        .radius(look.radius.lg)
        .border(1.0, look.rule)
        .child(heading)
        .children(progress)
        .children(failure)
        .children(not_connected)
}

/// The kit fills a progress bar with its active border; the design fills it with the accent.
pub(super) fn progress_kit(look: &Look) -> rune_kit::Kit {
    let mut kit = look.k.clone();
    kit.c.border_active = look.primary;
    kit
}
