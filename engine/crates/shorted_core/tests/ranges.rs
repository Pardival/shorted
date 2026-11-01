//! Ripple delete, split all and mute range (T-0109).

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use shorted_core::command::Command;
use shorted_core::evaluate::evaluate;
use shorted_core::model::*;
use uuid::Uuid;

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

const SEED: u128 = 0xABCD_0000;

/// 3 clips of 4 s at 1×, no transition, an audio item and an overlay crossing them.
fn project() -> Project {
    let mut project =
        Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap();
    project.timeline.transitions.clear();
    let mut clip = project.timeline.clips[0].clone();
    clip.speed = 1.0;
    project.timeline.clips = [20u128, 21, 22]
        .map(|value| ClipItem {
            id: id(value),
            ..clip.clone()
        })
        .to_vec();
    project.timeline.audio.truncate(1);
    let item = &mut project.timeline.audio[0];
    item.source_out_us = 9_000_000;
    item.start_us = 1_000_000;
    project.timeline.overlays.truncate(1);
    let overlay = &mut project.timeline.overlays[0];
    overlay.start_us = 3_000_000;
    overlay.end_us = 9_000_000;
    overlay.path = None;
    overlay.keyframes = vec![AnimatedProperty {
        property: Prop::X,
        keys: vec![
            Key {
                t_us: 0,
                value: 1.0,
                easing: Easing::Linear,
            },
            Key {
                t_us: 5_000_000,
                value: 2.0,
                easing: Easing::Linear,
            },
        ],
    }];
    project.validate().unwrap();
    project
}

fn delete(start_us: i64, end_us: i64) -> Command {
    Command::DeleteRange {
        start_us,
        end_us,
        id_seed: id(SEED),
    }
}

fn with_transition() -> Project {
    let mut project = project();
    project.timeline.transitions = vec![Transition {
        left: id(20),
        right: id(21),
        kind: TransitionKind::Crossfade,
        duration_us: 1_000_000,
    }];
    project.validate().unwrap();
    project
}

proptest! {
    #![proptest_config(ProptestConfig { max_global_rejects: 100_000, ..ProptestConfig::with_cases(300) })]

    #[test]
    fn delete_range_then_undo_restores_the_project(
        start in 0i64..11_000_000,
        length in 1i64..12_000_000,
        transition in any::<bool>(),
    ) {
        let original = if transition { with_transition() } else { project() };
        let mut edited = original.clone();
        let inverse = delete(start, start + length).apply(&mut edited).unwrap();
        inverse.apply(&mut edited).unwrap();
        prop_assert_eq!(edited, original);
    }

    #[test]
    fn everything_after_the_range_shifts_uniformly(
        start in 0i64..11_000_000,
        length in 1i64..3_000_000,
        probe in 0i64..12_000_000,
        transition in any::<bool>(),
    ) {
        let original = if transition { with_transition() } else { project() };
        let end = start + length;
        let total = original.duration_us();
        prop_assume!(end + probe < total);
        // Inside a transition cut by the range, either clip may fill the rest of the overlap.
        prop_assume!(!transition || end + probe >= 4_000_000 || end <= 3_000_000);
        let mut edited = original.clone();
        delete(start, end).apply(&mut edited).unwrap();
        prop_assert_eq!(edited.duration_us(), total - length.min(total - start));

        let before = evaluate(&original, end + probe);
        let after = evaluate(&edited, start + probe);
        let sources = |frame: &shorted_core::evaluate::FrameDescription| {
            frame.clips.last().map(|c| c.source_time_us)
        };
        prop_assert_eq!(sources(&before), sources(&after));
        prop_assert_eq!(before.overlays.len(), after.overlays.len());
        for (a, b) in before.overlays.iter().zip(&after.overlays) {
            prop_assert!((a.position.x - b.position.x).abs() < 1e-3);
        }
    }

    #[test]
    fn nothing_before_the_range_changes(start in 1i64..11_000_000, length in 1i64..3_000_000, t in 0i64..11_000_000) {
        prop_assume!(t < start);
        let original = project();
        let mut edited = original.clone();
        delete(start, start + length).apply(&mut edited).unwrap();
        let (before, after) = (evaluate(&original, t), evaluate(&edited, t));
        prop_assert_eq!(before.clips[0].source_time_us, after.clips[0].source_time_us);
        prop_assert_eq!(before.overlays.len(), after.overlays.len());
    }

    #[test]
    fn audio_on_a_track_never_overlaps(start in 0i64..11_000_000, length in 1i64..5_000_000, split in 0i64..11_000_000) {
        let mut edited = project();
        // `apply` validates, and validation refuses overlaps.
        Command::SplitAllAt { t_us: split, id_seed: id(SEED) }.apply(&mut edited).unwrap();
        Command::MuteRange { start_us: start, end_us: start + length, id_seed: id(SEED + 50) }
            .apply(&mut edited)
            .unwrap();
        Command::DeleteRange { start_us: start, end_us: start + length, id_seed: id(SEED + 100) }
            .apply(&mut edited)
            .unwrap();
    }

    #[test]
    fn split_all_changes_no_frame(t in 0i64..12_000_000, probe in 0i64..11_000_000) {
        let original = project();
        let mut edited = original.clone();
        Command::SplitAllAt { t_us: t, id_seed: id(SEED) }.apply(&mut edited).unwrap();
        let (before, after) = (evaluate(&original, probe), evaluate(&edited, probe));
        prop_assert_eq!(before.clips[0].source_time_us, after.clips[0].source_time_us);
        prop_assert_eq!(before.overlays.len(), after.overlays.len());
    }
}

#[test]
fn delete_inside_one_clip_splits_it() {
    let mut project = project();
    delete(1_000_000, 2_000_000).apply(&mut project).unwrap();
    let clips = &project.timeline.clips;
    assert_eq!(clips.len(), 4);
    assert_eq!(
        (clips[0].source_in_us, clips[0].source_out_us),
        (0, 1_000_000)
    );
    assert_eq!(
        (clips[1].source_in_us, clips[1].source_out_us),
        (2_000_000, 4_000_000)
    );
    assert_eq!(clips[1].id, id(SEED));
}

#[test]
fn a_clip_fully_inside_the_range_disappears() {
    let mut project = project();
    delete(4_000_000, 8_000_000).apply(&mut project).unwrap();
    assert_eq!(project.timeline.clips.len(), 2);
    assert_eq!(project.timeline.clips[1].id, id(22));
}

#[test]
fn speed_is_respected_when_cutting() {
    let mut project = project();
    project.timeline.clips[0].speed = 2.0;
    // The first clip lasts 2 s at 2×: cutting its second half removes 2 s of source.
    delete(1_000_000, 2_000_000).apply(&mut project).unwrap();
    assert_eq!(project.timeline.clips[0].source_out_us, 2_000_000);
    assert_eq!(project.timeline.clips[1].id, id(21));
}

#[test]
fn a_transition_touching_the_range_is_dropped() {
    let mut project = with_transition();
    delete(3_500_000, 5_000_000).apply(&mut project).unwrap();
    assert!(project.timeline.transitions.is_empty());
}

#[test]
fn a_transition_outside_the_range_survives_and_follows_a_split() {
    let mut project = with_transition();
    delete(1_000_000, 2_000_000).apply(&mut project).unwrap();
    assert_eq!(project.timeline.transitions.len(), 1);
    assert_eq!(project.timeline.transitions[0].left, id(SEED));
    assert_eq!(project.timeline.transitions[0].right, id(21));
}

#[test]
fn keyframes_keep_relative_timing_and_cuts_do_not_jump() {
    let mut project = project();
    delete(4_000_000, 5_000_000).apply(&mut project).unwrap();
    let overlays = &project.timeline.overlays;
    assert_eq!(overlays.len(), 2);
    assert_eq!(
        (overlays[1].start_us, overlays[1].end_us),
        (4_000_000, 8_000_000)
    );
    let keys = &overlays[1].keyframes[0].keys;
    // A key at the cut, holding the value reached there, then the original key.
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].t_us, 0);
    assert!((keys[0].value - 1.4).abs() < 1e-6);
    assert_eq!(keys[1].t_us, 3_000_000);
}

#[test]
fn a_key_exactly_on_the_cut_is_not_duplicated() {
    let mut project = project();
    delete(7_000_000, 8_000_000).apply(&mut project).unwrap();
    assert_eq!(project.timeline.overlays[1].keyframes[0].keys[0].t_us, 0);
}

#[test]
fn a_motion_path_does_not_jump_at_a_cut() {
    let mut project = project();
    project.timeline.overlays[0].path = Some(MotionPath {
        samples: vec![
            PathSample {
                t_us: 0,
                x: 0.0,
                y: 0.0,
            },
            PathSample {
                t_us: 6_000_000,
                x: 600.0,
                y: 0.0,
            },
        ],
        smoothing: 0.0,
        orient_to_path: false,
    });
    project.timeline.overlays[0].keyframes.clear();
    delete(4_000_000, 5_000_000).apply(&mut project).unwrap();
    let frame = evaluate(&project, 4_000_000);
    assert_eq!(frame.overlays[0].position.x, 200.0);
}

#[test]
fn a_video_element_keeps_its_place_in_the_source_when_cut() {
    let mut project = project();
    project.timeline.overlays[0].kind = OverlayKind::Video {
        media: id(10),
        source_in_us: 0,
    };
    delete(4_000_000, 5_000_000).apply(&mut project).unwrap();
    let frame = evaluate(&project, 4_000_000);
    assert_eq!(frame.overlays[0].source_time_us, Some(2_000_000));
}

#[test]
fn locked_tracks_are_untouched() {
    let original = project();

    let mut locked_video = original.clone();
    locked_video.timeline.track_flags.video.locked = true;
    delete(0, 1_000_000).apply(&mut locked_video).unwrap();
    assert_eq!(locked_video.timeline.clips, original.timeline.clips);

    let mut locked_widgets = original.clone();
    locked_widgets.timeline.track_flags.widgets.locked = true;
    delete(0, 1_000_000).apply(&mut locked_widgets).unwrap();
    assert_eq!(locked_widgets.timeline.overlays, original.timeline.overlays);

    let mut locked_voice = original.clone();
    locked_voice.timeline.track_flags.voice_over.locked = true;
    delete(0, 2_000_000).apply(&mut locked_voice).unwrap();
    assert_eq!(locked_voice.timeline.audio, original.timeline.audio);

    let mut locked_item = original.clone();
    locked_item.timeline.overlays[0].locked = true;
    let expected = locked_item.timeline.overlays.clone();
    delete(0, 5_000_000).apply(&mut locked_item).unwrap();
    assert_eq!(locked_item.timeline.overlays, expected);
}

#[test]
fn deleting_an_item_removes_its_caption_source() {
    let mut project = project();
    delete(1_000_000, 10_000_000).apply(&mut project).unwrap();
    assert!(project.timeline.audio.is_empty());
    assert_eq!(project.captions.sources, [CaptionSource::OriginalSound]);
}

#[test]
fn mute_range_silences_only_the_range() {
    let mut project = project();
    Command::MuteRange {
        start_us: 2_000_000,
        end_us: 3_000_000,
        id_seed: id(SEED),
    }
    .apply(&mut project)
    .unwrap();
    let audio = &project.timeline.audio;
    assert_eq!(audio.len(), 3);
    assert_eq!(audio[0].volume, 1.0);
    assert_eq!((audio[1].volume, audio[1].start_us), (0.0, 2_000_000));
    assert_eq!((audio[2].volume, audio[2].start_us), (1.0, 3_000_000));
}

#[test]
fn an_empty_or_reversed_range_is_refused() {
    let mut project = project();
    assert!(delete(5, 5).apply(&mut project).is_err());
    assert!(delete(9, 5).apply(&mut project).is_err());
    assert!(delete(-1, 5).apply(&mut project).is_err());
}
