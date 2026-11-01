//! Boundaries and continuity of timeline evaluation (T-0103).

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use shorted_core::evaluate::{ease, evaluate};
use shorted_core::model::*;
use uuid::Uuid;

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn fixture() -> Project {
    Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap()
}

/// Clips of 4 s and 3 s at 1×, with a 1 s crossfade.
fn two_clips() -> Project {
    let mut project = fixture();
    project.timeline.audio.clear();
    project.timeline.overlays.clear();
    project.captions.sources = vec![CaptionSource::OriginalSound];
    let clips = &mut project.timeline.clips;
    clips[0].speed = 1.0;
    clips[1].speed = 1.0;
    clips[1].source_out_us = 3_000_000;
    clips[1].image_motion = None;
    clips[1].media_id = id(10);
    project.timeline.transitions[0].duration_us = 1_000_000;
    project.timeline.transitions[0].kind = TransitionKind::Crossfade;
    project.validate().unwrap();
    project
}

fn single_clip(speed: f32) -> Project {
    let mut project = two_clips();
    project.timeline.transitions.clear();
    project.timeline.clips.truncate(1);
    project.timeline.clips[0].speed = speed;
    project
}

fn overlay_only(change: impl FnOnce(&mut OverlayItem)) -> Project {
    let mut project = fixture();
    project.timeline.overlays.truncate(1);
    let item = &mut project.timeline.overlays[0];
    item.keyframes.clear();
    item.path = None;
    item.anim_in = None;
    item.anim_loop = None;
    item.anim_out = None;
    change(item);
    project
}

#[test]
fn empty_project_gives_an_empty_frame() {
    let project = Project::new(id(1), "x".into(), "now".into());
    let frame = evaluate(&project, 5);
    assert!(frame.clips.is_empty() && frame.overlays.is_empty());
    assert_eq!(frame.time_us, 0);
}

#[test]
fn clip_edges_are_half_open() {
    let project = single_clip(1.0);
    let first = evaluate(&project, 0);
    assert_eq!(first.clips[0].source_time_us, 0);
    let last = evaluate(&project, 3_999_999);
    assert_eq!(last.clips[0].source_time_us, 3_999_999);
}

#[test]
fn time_is_clamped_to_the_project() {
    let project = single_clip(1.0);
    assert_eq!(evaluate(&project, -5).time_us, 0);
    let past = evaluate(&project, 99_000_000);
    assert_eq!(past.time_us, 3_999_999);
    assert_eq!(past.clips.len(), 1);
}

#[test]
fn trim_offsets_source_time() {
    let mut project = single_clip(1.0);
    project.timeline.clips[0].source_in_us = 1_000_000;
    assert_eq!(
        evaluate(&project, 500_000).clips[0].source_time_us,
        1_500_000
    );
}

#[test]
fn speed_scales_source_time() {
    let fast = single_clip(2.0);
    assert_eq!(
        evaluate(&fast, 1_000_000).clips[0].source_time_us,
        2_000_000
    );
    let slow = single_clip(0.5);
    assert_eq!(
        evaluate(&slow, 2_000_000).clips[0].source_time_us,
        1_000_000
    );
    // At 2× a 4 s source lasts 2 s: the last instant never reads past the source.
    let end = evaluate(&fast, 99_000_000);
    assert!(end.clips[0].source_time_us < 4_000_000);
}

#[test]
fn transition_overlap_has_two_layers_in_order() {
    let project = two_clips();
    assert_eq!(project.duration_us(), 6_000_000);
    let before = evaluate(&project, 2_999_999);
    assert_eq!(before.clips.len(), 1);
    assert!(before.transition.is_none());

    let start = evaluate(&project, 3_000_000);
    assert_eq!(start.clips.len(), 2);
    assert_eq!(start.clips[0].clip_id, id(20));
    assert_eq!(start.clips[1].clip_id, id(21));
    assert_eq!(start.clips[1].source_time_us, 0);
    assert_eq!(start.transition.unwrap().progress, 0.0);

    let middle = evaluate(&project, 3_500_000);
    assert_eq!(middle.transition.unwrap().progress, 0.5);
    assert_eq!(middle.clips[0].source_time_us, 3_500_000);
    assert_eq!(middle.clips[1].source_time_us, 500_000);

    let after = evaluate(&project, 4_000_000);
    assert_eq!(after.clips.len(), 1);
    assert_eq!(after.clips[0].clip_id, id(21));
    assert!(after.transition.is_none());
}

#[test]
fn clips_without_transition_are_contiguous() {
    let mut project = two_clips();
    project.timeline.transitions.clear();
    assert_eq!(evaluate(&project, 3_999_999).clips[0].clip_id, id(20));
    let next = evaluate(&project, 4_000_000);
    assert_eq!(next.clips.len(), 1);
    assert_eq!(
        (next.clips[0].clip_id, next.clips[0].source_time_us),
        (id(21), 0)
    );
}

#[test]
fn ken_burns_runs_across_the_clip() {
    let mut project = fixture();
    project.timeline.transitions.clear();
    project.timeline.clips.remove(0);
    let clip = &mut project.timeline.clips[0];
    clip.speed = 1.0;
    clip.crop.scale = 1.0;
    let start = evaluate(&project, 0);
    let middle = evaluate(&project, 2_000_000);
    assert_eq!(start.clips[0].crop.scale, 1.0);
    assert!((middle.clips[0].crop.scale - 1.1).abs() < 1e-5);
}

#[test]
fn hidden_tracks_and_items_are_left_out() {
    let mut project = fixture();
    assert_eq!(evaluate(&project, 0).overlays.len(), 5);
    project.timeline.overlays[0].hidden = true;
    assert_eq!(evaluate(&project, 0).overlays.len(), 4);
    project.timeline.track_flags.widgets.hidden = true;
    assert!(evaluate(&project, 0).overlays.is_empty());
    project.timeline.track_flags.video.hidden = true;
    assert!(evaluate(&project, 0).clips.is_empty());
}

#[test]
fn overlay_range_is_half_open_and_ordered() {
    let project = fixture();
    let frame = evaluate(&project, 1_999_999);
    let order: Vec<Id> = frame.overlays.iter().map(|o| o.overlay_id).collect();
    assert_eq!(order, [id(50), id(51), id(52), id(53), id(54)]);
    assert!(evaluate(&project, 2_000_000).overlays.is_empty());
}

#[test]
fn video_elements_report_their_source_time() {
    let frame = evaluate(&fixture(), 500_000);
    assert_eq!(frame.overlays[2].source_time_us, Some(500_000));
    assert_eq!(frame.overlays[0].source_time_us, None);
}

#[test]
fn keyframes_interpolate_and_hold() {
    let project = overlay_only(|item| {
        item.keyframes = vec![AnimatedProperty {
            property: Prop::X,
            keys: vec![
                Key {
                    t_us: 500_000,
                    value: 100.0,
                    easing: Easing::Linear,
                },
                Key {
                    t_us: 1_500_000,
                    value: 200.0,
                    easing: Easing::Linear,
                },
            ],
        }];
    });
    let x = |t| evaluate(&project, t).overlays[0].position.x;
    assert_eq!(x(0), 100.0);
    assert_eq!(x(1_000_000), 150.0);
    assert_eq!(x(1_900_000), 200.0);
}

#[test]
fn keyframes_win_over_the_path_on_their_axis() {
    let project = overlay_only(|item| {
        item.path = Some(MotionPath {
            samples: vec![
                PathSample {
                    t_us: 0,
                    x: 0.0,
                    y: 0.0,
                },
                PathSample {
                    t_us: 1_000_000,
                    x: 100.0,
                    y: 200.0,
                },
            ],
            smoothing: 0.0,
            orient_to_path: false,
        });
        item.keyframes = vec![AnimatedProperty {
            property: Prop::Y,
            keys: vec![Key {
                t_us: 0,
                value: 7.0,
                easing: Easing::Linear,
            }],
        }];
    });
    let position = evaluate(&project, 500_000).overlays[0].position;
    assert_eq!((position.x, position.y), (50.0, 7.0));
}

#[test]
fn presets_are_relative_and_end_neutral() {
    let project = overlay_only(|item| {
        item.base.opacity = 0.8;
        item.anim_in = Some(PresetAnim {
            kind: PresetKind::Fade,
            duration_us: 1_000_000,
        });
    });
    let opacity = |t| evaluate(&project, t).overlays[0].opacity;
    assert_eq!(opacity(0), 0.0);
    assert!((opacity(500_000) - 0.4).abs() < 1e-6);
    assert!((opacity(1_000_000) - 0.8).abs() < 1e-6);
}

#[test]
fn out_preset_finishes_at_the_end_of_the_item() {
    let project = overlay_only(|item| {
        item.anim_out = Some(PresetAnim {
            kind: PresetKind::Fade,
            duration_us: 1_000_000,
        });
    });
    assert!(evaluate(&project, 1_999_999).overlays[0].opacity < 0.001);
    assert!(evaluate(&project, 1_000_000).overlays[0].opacity > 0.79);
}

#[test]
fn captions_follow_source_time_and_flags() {
    let mut project = two_clips();
    project.timeline.transitions.clear();
    let frame = evaluate(&project, 100_000);
    assert_eq!(frame.captions.len(), 1);
    assert!(frame.captions[0].emphasis);
    assert!(evaluate(&project, 450_000).captions.is_empty());
    project.timeline.clips[0]
        .transcript
        .as_mut()
        .unwrap()
        .segments[0]
        .hidden = true;
    assert!(evaluate(&project, 100_000).captions.is_empty());
    project.timeline.clips[0]
        .transcript
        .as_mut()
        .unwrap()
        .segments[0]
        .hidden = false;
    project.captions.enabled = false;
    assert!(evaluate(&project, 100_000).captions.is_empty());
}

#[test]
fn easings_start_at_zero_and_end_at_one() {
    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
        Easing::Spring,
    ] {
        assert!(ease(easing, 0.0).abs() < 1e-6, "{easing:?}");
        assert!((ease(easing, 1.0) - 1.0).abs() < 1e-6, "{easing:?}");
    }
}

proptest! {
    #[test]
    fn evaluation_is_deterministic(t in -1_000_000i64..10_000_000) {
        let project = fixture();
        prop_assert_eq!(evaluate(&project, t), evaluate(&project, t));
    }

    #[test]
    fn source_time_advances_continuously_inside_a_clip(t in 0i64..3_900_000, speed in 0.5f32..2.0) {
        let project = single_clip(speed);
        prop_assume!(t + 2_000 < project.duration_us());
        let first = evaluate(&project, t).clips[0].source_time_us;
        let second = evaluate(&project, t + 1_000).clips[0].source_time_us;
        let step = second - first;
        prop_assert!((500..=2_001).contains(&step), "step {}", step);
    }

    #[test]
    fn overlay_moves_continuously(t in 0i64..1_990_000, kind in prop::sample::select(vec![
        Easing::Linear, Easing::EaseIn, Easing::EaseOut, Easing::EaseInOut, Easing::Spring,
    ])) {
        let project = overlay_only(|item| {
            item.keyframes = vec![AnimatedProperty {
                property: Prop::X,
                keys: vec![
                    Key { t_us: 0, value: 0.0, easing: kind },
                    Key { t_us: 2_000_000, value: 1000.0, easing: kind },
                ],
            }];
        });
        let a = evaluate(&project, t).overlays[0].position.x;
        let b = evaluate(&project, t + 1_000).overlays[0].position.x;
        prop_assert!((b - a).abs() < 12.0, "jump {} at {}", b - a, t);
    }

    #[test]
    fn every_time_inside_the_project_shows_a_clip(t in 0i64..6_000_000) {
        let project = two_clips();
        let frame = evaluate(&project, t);
        prop_assert!(!frame.clips.is_empty());
        prop_assert!(frame.clips.len() <= 2);
    }

    #[test]
    fn durations_agree_with_evaluation(speed in 0.5f32..2.0) {
        let project = single_clip(speed);
        let frame = evaluate(&project, i64::MAX);
        prop_assert_eq!(frame.time_us, project.duration_us() - 1);
    }
}
