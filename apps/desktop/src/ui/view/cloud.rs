//! A photo's place in the cloud: the inspector's section with its action (Upload, Publish,
//! Unpublish or Retry), whether the public can see it, and its description; and the mark a
//! grid tile carries. Each state has its own icon and words, so no state is told by colour
//! alone.

use super::super::{
    format,
    gallery::{Gallery, GalleryPhoto, description_length, failure_of},
    icons,
    look::Look,
};
use crate::{
    DESCRIPTION_LIMIT,
    api::Connection,
    cloud::{CloudState, CloudStep, ObjectKind},
};
use rune::{Color, El, Icon, TextStyle, col, icon, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, Progress, TextArea};

const MARK_SIZE: f32 = 20.0;
const MARK_INSET: f32 = 6.0;

fn state_icon(state: &CloudState) -> &'static Icon {
    match state {
        CloudState::Local => &icons::CLOUD_OFF,
        CloudState::Queued | CloudState::Publishing { .. } | CloudState::Unpublishing { .. } => {
            &icons::CLOCK
        }
        CloudState::Uploading { .. } => &icons::CLOUD_UPLOAD,
        CloudState::Draft { .. } => &icons::CLOUD_CHECK,
        CloudState::Published { .. } => &icons::GLOBE,
        CloudState::Failed { .. } => &icons::ALERT,
    }
}

fn state_color(state: &CloudState, look: &Look) -> Color {
    match state {
        CloudState::Failed { .. } => look.warning,
        CloudState::Published { .. } => look.fg,
        _ => look.muted,
    }
}

/// The mark in a tile's top-right corner. A photo only in the library carries none. A published
/// photo's globe differs in shape from a draft's cloud, and the label names each state.
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

/// The one action the inspector offers for a state.
struct Action {
    label: &'static str,
    key: &'static str,
    help: &'static str,
    icon: &'static Icon,
    press: fn(&mut Gallery),
}

fn action(state: &CloudState) -> Option<Action> {
    let retry = |key, help| Action {
        label: "Retry",
        key,
        help,
        icon: &icons::CLOUD_UPLOAD,
        press: Gallery::retry_selected,
    };
    Some(match state {
        CloudState::Local => Action {
            label: "Upload",
            key: "upload-photo",
            help: "Upload as a private draft (⌘U)",
            icon: &icons::CLOUD_UPLOAD,
            press: Gallery::upload_selected,
        },
        CloudState::Draft { .. } => Action {
            label: "Publish",
            key: "publish-photo",
            help: "Add it to the public gallery (⌘P)",
            icon: &icons::GLOBE,
            press: Gallery::publish_selected,
        },
        CloudState::Published { .. } => Action {
            label: "Unpublish",
            key: "unpublish-photo",
            help: "Take it out of the public gallery (⇧⌘P)",
            icon: &icons::CLOUD_CHECK,
            press: Gallery::unpublish_selected,
        },
        CloudState::Failed { .. } => match state.retry()? {
            CloudStep::Upload => retry("retry-upload", "Try the upload again (⌘U)"),
            CloudStep::Publish => retry("retry-publish", "Try publishing again (⌘P)"),
            CloudStep::Unpublish => retry("retry-unpublish", "Try unpublishing again (⇧⌘P)"),
        },
        _ => return None,
    })
}

/// Who can see the photo, in words, while the cloud holds it.
fn audience(state: &CloudState) -> Option<&'static str> {
    Some(match state {
        CloudState::Draft { .. } => "Private. Only you can see it.",
        CloudState::Publishing { .. } => "Adding it to the public gallery.",
        CloudState::Published { .. } => "Public. Anyone can see it in the gallery.",
        CloudState::Unpublishing { .. } => {
            "Taking it out of the public gallery. Visitors may see it until this finishes."
        }
        CloudState::Failed {
            step: CloudStep::Unpublish,
            ..
        } => "It may still be public.",
        _ => return None,
    })
}

/// The selected photo's cloud state, what to do about it, who can see it, its description,
/// and why something can't happen yet.
pub(super) fn cloud_section(s: &Gallery, photo: &GalleryPhoto, look: &Look) -> El {
    let state = &photo.record.cloud;
    let connected = s.is_connected();
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
        .children(action(state).map(|action| {
            let press = action.press;
            Button::new(action.label)
                .icon(action.icon)
                .control_size(ControlSize::Small)
                .disabled(!connected)
                .help(action.help)
                .on_press(move |s: &mut Gallery| press(s))
                .build(look)
                .key(action.key)
        }));
    let audience = audience(state).map(|words| {
        text(words)
            .key("public-state")
            .font(look.ty.small)
            .color(look.muted)
            .wrap()
    });
    let progress = match state {
        CloudState::Uploading { missing, .. } => {
            #[allow(clippy::cast_precision_loss)]
            let value =
                (ObjectKind::ALL.len() - missing.len()) as f32 / ObjectKind::ALL.len() as f32;
            Some(Progress::new(Some(value)))
        }
        CloudState::Queued | CloudState::Publishing { .. } | CloudState::Unpublishing { .. } => {
            Some(Progress::new(None))
        }
        _ => None,
    }
    .map(|progress| {
        progress
            .build(&progress_kit(look))
            .key("photo-cloud-progress")
    });
    let failure = failure_of(state).map(|(failure, _)| {
        text(failure.message())
            .key("cloud-failure")
            .role("alert")
            .font(look.ty.small)
            .color(look.muted)
            .wrap()
    });
    let waiting = state.is_busy()
        || CloudStep::ALL
            .into_iter()
            .any(|step| state.start(step).is_some());
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
        .children(audience)
        .children(progress)
        .children(failure)
        .children(not_connected)
        .child(description(s, photo, look))
}

/// The description a visitor's screen reader says for the photo: a field, how much of the
/// limit it uses, and Save while it differs from what is kept.
fn description(s: &Gallery, photo: &GalleryPhoto, look: &Look) -> El {
    let edit = s.description_edit(photo);
    let length = description_length(&edit.text);
    let too_long = length > DESCRIPTION_LIMIT;
    let changed = s.description_changed(photo);
    let published = matches!(photo.record.cloud, CloudState::Published { .. });
    let save_help = if published {
        "Save, and publish again with the new words (⌘↩)"
    } else {
        "Save the description (⌘↩)"
    };
    let header = row()
        .w_full()
        .center_y()
        .child(
            text("Description")
                .font(look.ty.small_medium)
                .color(look.muted)
                .grow(1.0),
        )
        .child(
            text(format!("{length} / {DESCRIPTION_LIMIT}"))
                .key("description-count")
                .font(TextStyle {
                    features: "'tnum' 1",
                    ..look.ty.small
                })
                .color(if too_long { look.warning } else { look.faint }),
        );
    let field = TextArea::new(&edit)
        .label("Description")
        .placeholder("What a visitor who can't see the photo should hear")
        .width(None)
        .auto_grow(2, 6)
        .invalid(too_long)
        .on_edit(|s: &mut Gallery| s.description_mut())
        .on_submit(|s: &mut Gallery, _| s.save_description())
        .build(look)
        .key("description");
    let note = too_long.then(|| {
        text(format!(
            "The public gallery takes up to {DESCRIPTION_LIMIT} characters."
        ))
        .key("description-too-long")
        .role("alert")
        .font(look.ty.small)
        .color(look.muted)
        .wrap()
    });
    let save = changed.then(|| {
        row().w_full().justify_end().child(
            Button::new("Save")
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .disabled(too_long)
                .help(save_help)
                .on_press(|s: &mut Gallery| s.save_description())
                .build(look)
                .key("save-description"),
        )
    });
    col()
        .key("description-section")
        .w_full()
        .gap(4.0)
        .pt(6.0)
        .child(header)
        .child(field)
        .children(note)
        .children(save)
}

/// The kit fills a progress bar with its active border; the design fills it with the accent.
pub(super) fn progress_kit(look: &Look) -> rune_kit::Kit {
    let mut kit = look.k.clone();
    kit.c.border_active = look.primary;
    kit
}
