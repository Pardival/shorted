//! Boundaries and continuity of timeline evaluation (T-0103).

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use shorted_core::command::AnimSlot;
use shorted_core::evaluate::{
    Modifier, ease, evaluate, keyframe_times, keys_for, preset_modifier, property_value,
    simplify_path,
};
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

fn spoken(texts: &[(&str, i64, i64)]) -> Transcript {
    Transcript {
        language: "en".into(),
        segments: vec![Segment {
            id: Uuid::from_u128(900),
            hidden: false,
            words: texts
                .iter()
                .map(|(text, start_us, end_us)| Word {
                    text: (*text).into(),
                    original_text: (*text).into(),
                    start_us: *start_us,
                    end_us: *end_us,
                    confidence: 1.0,
                    emphasis: EmphasisState::Auto(false),
                    deleted: false,
                })
                .collect(),
        }],
    }
}

#[test]
fn caption_groups_stay_up_for_their_words_and_mark_the_active_one() {
    let mut project = two_clips();
    project.timeline.transitions.clear();
    project.timeline.clips[0].transcript = Some(spoken(&[
        ("one", 0, 100_000),
        ("two", 100_000, 200_000),
        ("three", 200_000, 300_000),
    ]));
    project.captions.grouping = Grouping::UpTo(2);
    let frame = evaluate(&project, 150_000);
    let texts: Vec<_> = frame.captions.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, ["one", "two"]);
    assert!(!frame.captions[0].active && frame.captions[1].active);
    assert_eq!(frame.caption_age_us, 150_000);

    let next = evaluate(&project, 250_000);
    assert_eq!(next.captions.len(), 1);
    assert_eq!(next.caption_age_us, 50_000);

    project.captions.grouping = Grouping::Sentence;
    assert_eq!(evaluate(&project, 250_000).captions.len(), 3);
}

#[test]
fn captions_follow_source_time_and_flags() {
    let mut project = two_clips();
    project.timeline.transitions.clear();
    project.captions.grouping = Grouping::OneWord;
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

const IN_PRESETS: [PresetKind; 9] = [
    PresetKind::Pop,
    PresetKind::SlideLeft,
    PresetKind::SlideRight,
    PresetKind::SlideUp,
    PresetKind::SlideDown,
    PresetKind::Fade,
    PresetKind::Zoom,
    PresetKind::Stamp,
    PresetKind::Wipe,
];

fn hidden(modifier: Modifier) -> bool {
    modifier.opacity < 0.01
        || modifier.scale < 0.01
        || modifier.reveal < 0.01
        || modifier.offset.x.abs() >= 500.0
        || modifier.offset.y.abs() >= 500.0
}

fn neutral(modifier: Modifier) -> bool {
    (modifier.opacity - 1.0).abs() < 1e-4
        && (modifier.scale - 1.0).abs() < 1e-3
        && (modifier.reveal - 1.0).abs() < 1e-4
        && modifier.offset.x.abs() < 1e-3
        && modifier.offset.y.abs() < 1e-3
        && modifier.rotation.abs() < 1e-3
}

#[test]
fn every_in_preset_arrives_and_every_out_preset_leaves() {
    for kind in IN_PRESETS {
        assert!(
            hidden(preset_modifier(kind, AnimSlot::In, 0.0)),
            "{kind:?} starts visible"
        );
        assert!(
            neutral(preset_modifier(kind, AnimSlot::In, 1.0)),
            "{kind:?} does not settle"
        );
    }
    for kind in [
        PresetKind::Fade,
        PresetKind::Shrink,
        PresetKind::SlideLeft,
        PresetKind::Pop,
    ] {
        assert!(
            neutral(preset_modifier(kind, AnimSlot::Out, 0.0)),
            "{kind:?}"
        );
        assert!(
            hidden(preset_modifier(kind, AnimSlot::Out, 1.0)),
            "{kind:?}"
        );
    }
}

#[test]
fn every_loop_comes_back_to_where_it_started() {
    for kind in [
        PresetKind::Float,
        PresetKind::Pulse,
        PresetKind::Wiggle,
        PresetKind::Bounce,
    ] {
        let start = preset_modifier(kind, AnimSlot::Loop, 0.0);
        let end = preset_modifier(kind, AnimSlot::Loop, 1.0);
        assert!(neutral(start), "{kind:?}");
        assert!((start.offset.y - end.offset.y).abs() < 1e-3, "{kind:?}");
        assert!((start.scale - end.scale).abs() < 1e-3, "{kind:?}");
        assert!(
            !neutral(preset_modifier(kind, AnimSlot::Loop, 0.25)),
            "{kind:?} does not move"
        );
    }
    let turned = preset_modifier(PresetKind::Rotate, AnimSlot::Loop, 0.5);
    assert_eq!(turned.rotation, 180.0);
}

#[test]
fn a_wipe_reveals_from_the_left() {
    let project = overlay_only(|item| {
        item.anim_in = Some(PresetAnim {
            kind: PresetKind::Wipe,
            duration_us: 1_000_000,
        });
    });
    let reveal = |t| evaluate(&project, t).overlays[0].reveal;
    assert_eq!(reveal(0), 0.0);
    assert!((reveal(500_000) - 0.5).abs() < 1e-3);
    assert_eq!(reveal(1_000_000), 1.0);
}

#[test]
fn a_preset_adds_to_keyframes() {
    let project = overlay_only(|item| {
        item.keyframes = vec![AnimatedProperty {
            property: Prop::X,
            keys: vec![
                Key {
                    t_us: 0,
                    value: 100.0,
                    easing: Easing::Linear,
                },
                Key {
                    t_us: 1_000_000,
                    value: 300.0,
                    easing: Easing::Linear,
                },
            ],
        }];
        item.anim_in = Some(PresetAnim {
            kind: PresetKind::SlideLeft,
            duration_us: 1_000_000,
        });
    });
    let x = |t| evaluate(&project, t).overlays[0].position.x;
    let slide = preset_modifier(PresetKind::SlideLeft, AnimSlot::In, 0.5)
        .offset
        .x;
    assert!((x(500_000) - (200.0 + slide)).abs() < 1e-3);
    assert!((x(1_000_000) - 300.0).abs() < 1e-3);
}

#[test]
fn a_loop_adds_to_a_path_and_its_heading() {
    let project = overlay_only(|item| {
        item.base.rotation = 0.0;
        item.path = Some(MotionPath {
            samples: vec![
                PathSample {
                    t_us: 0,
                    x: 0.0,
                    y: 0.0,
                },
                PathSample {
                    t_us: 2_000_000,
                    x: 0.0,
                    y: 400.0,
                },
            ],
            smoothing: 0.0,
            orient_to_path: true,
        });
        item.anim_loop = Some(PresetAnim {
            kind: PresetKind::Wiggle,
            duration_us: 1_000_000,
        });
    });
    let layer = evaluate(&project, 250_000).overlays[0].clone();
    let wiggle = preset_modifier(PresetKind::Wiggle, AnimSlot::Loop, 0.25).rotation;
    assert!((layer.position.y - 50.0).abs() < 1e-3);
    assert!(
        (layer.rotation - (90.0 + wiggle)).abs() < 1e-3,
        "{}",
        layer.rotation
    );
}

fn two_keys(easing: Easing) -> Project {
    overlay_only(|item| {
        item.keyframes = vec![AnimatedProperty {
            property: Prop::Opacity,
            keys: vec![
                Key {
                    t_us: 0,
                    value: 0.0,
                    easing,
                },
                Key {
                    t_us: 1_000_000,
                    value: 1.0,
                    easing: Easing::Linear,
                },
            ],
        }];
    })
}

#[test]
fn each_easing_shapes_the_way_between_two_keys() {
    let at = |easing, t| evaluate(&two_keys(easing), t).overlays[0].opacity;
    assert!((at(Easing::Linear, 250_000) - 0.25).abs() < 1e-4);
    assert!((at(Easing::EaseIn, 250_000) - 0.0625).abs() < 1e-4);
    assert!((at(Easing::EaseOut, 250_000) - 0.4375).abs() < 1e-4);
    assert!((at(Easing::EaseInOut, 250_000) - 0.15625).abs() < 1e-4);
    assert!((at(Easing::EaseInOut, 500_000) - 0.5).abs() < 1e-4);
    // A spring overshoots, so opacity is clamped at 1 on its way.
    assert!(at(Easing::Spring, 250_000) > at(Easing::EaseOut, 250_000));
    assert_eq!(ease(Easing::Spring, 1.0), 1.0);
    let overshoot = (1..100).any(|i| ease(Easing::Spring, i as f32 / 100.0) > 1.0);
    assert!(overshoot, "a spring overshoots before settling");
    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::Spring,
    ] {
        assert!((at(easing, 1_000_000) - 1.0).abs() < 1e-4, "{easing:?}");
    }
}

#[test]
fn a_change_at_the_playhead_keys_only_what_changed() {
    let project = two_keys(Easing::Spring);
    let item = &project.timeline.overlays[0];
    let mut target = item.base;
    target.opacity = property_value(item, Prop::Opacity, 0);
    target.position.x += 50.0;
    let keys = keys_for(item, 0, &target);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].0, Prop::X);
    assert_eq!(keys[0].1.easing, Easing::EaseInOut);

    target.opacity = 0.3;
    let keys = keys_for(item, 0, &target);
    let opacity = keys.iter().find(|(p, _)| *p == Prop::Opacity).unwrap();
    assert_eq!(
        opacity.1.easing,
        Easing::Spring,
        "an existing key keeps its easing"
    );
    assert_eq!(
        keyframe_times(item),
        vec![(0, Easing::Spring), (1_000_000, Easing::Linear)]
    );
}

fn sample(t_us: i64, x: f32, y: f32) -> PathSample {
    PathSample { t_us, x, y }
}

fn corner(smoothing: f32) -> MotionPath {
    MotionPath {
        samples: vec![
            sample(0, 0.0, 0.0),
            sample(1_000_000, 100.0, 0.0),
            sample(2_000_000, 100.0, 100.0),
        ],
        smoothing,
        orient_to_path: false,
    }
}

#[test]
fn smoothing_rounds_a_corner_but_keeps_the_samples() {
    let at = |smoothing, t| {
        let project = overlay_only(|item| item.path = Some(corner(smoothing)));
        evaluate(&project, t).overlays[0].position
    };
    let straight = at(0.0, 1_500_000);
    let smooth = at(1.0, 1_500_000);
    assert_eq!((straight.x, straight.y), (100.0, 50.0));
    assert!(
        smooth.x > 100.0,
        "the curve bulges out of the corner: {smooth:?}"
    );
    let half = at(0.5, 1_500_000);
    assert!(half.x > straight.x && half.x < smooth.x);
    for t in [0, 1_000_000] {
        let (a, b) = (at(0.0, t), at(1.0, t));
        assert!((a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3);
    }
}

#[test]
fn simplification_keeps_corners_and_drops_straight_runs() {
    let mut samples: Vec<PathSample> = (0..=10)
        .map(|i| sample(i * 100_000, i as f32 * 10.0, 0.0))
        .collect();
    samples.extend((1..=10).map(|i| sample(1_000_000 + i * 100_000, 100.0, i as f32 * 10.0)));
    samples[3].y = 0.5;
    let kept = simplify_path(&samples, 2.0);
    assert_eq!(
        kept.iter().map(|s| s.t_us).collect::<Vec<_>>(),
        vec![0, 1_000_000, 2_000_000]
    );
    assert_eq!(simplify_path(&samples[..2], 2.0).len(), 2);
}
