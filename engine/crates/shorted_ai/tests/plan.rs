//! Edit plans (T-0802): strict reading, aliases, validation, and all-or-nothing application.

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use shorted_ai::plan::*;
use shorted_core::command::Command;
use shorted_core::model::*;
use uuid::Uuid;

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn word(text: &str, start_us: i64) -> Word {
    Word {
        text: text.into(),
        original_text: text.into(),
        start_us,
        end_us: start_us + 300_000,
        confidence: 0.9,
        emphasis: EmphasisState::Auto(false),
        deleted: false,
    }
}

/// Three clips of 4 s from a 10 s video; the first one speaks.
fn project() -> Project {
    let mut project =
        Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap();
    project.timeline.audio.clear();
    project.timeline.overlays.clear();
    project.timeline.transitions.clear();
    project.captions.sources = vec![CaptionSource::OriginalSound];
    let video = project
        .media
        .iter_mut()
        .find(|media| media.kind == MediaKind::Video)
        .unwrap();
    video.probe.duration_us = 10_000_000;
    let video = video.id;
    let template = project.timeline.clips[0].clone();
    project.timeline.clips = (1..=3)
        .map(|n| ClipItem {
            id: id(100 + n),
            media_id: video,
            source_in_us: 0,
            source_out_us: 4_000_000,
            speed: 1.0,
            image_motion: None,
            transcript: None,
            ..template.clone()
        })
        .collect();
    project.timeline.clips[0].transcript = Some(Transcript {
        language: "en".into(),
        segments: vec![Segment {
            id: id(200),
            hidden: false,
            words: vec![
                word("coffee", 0),
                word("tastes", 400_000),
                word("great", 800_000),
            ],
        }],
    });
    project.validate().unwrap();
    project
}

fn ids() -> impl FnMut() -> Id {
    let mut next = 500;
    move || {
        next += 1;
        id(next)
    }
}

fn check(json: &str) -> Result<ValidPlan, Vec<String>> {
    let project = project();
    let plan = parse(json)?;
    validate(&plan, &project, &Aliases::of(&project), &mut ids())
}

#[test]
fn unknown_ops_and_fields_are_refused() {
    let unknown_op = r#"{"version":1,"source":"ai","ops":[{"op":"explode","clip":"c1"}]}"#;
    assert!(parse(unknown_op).is_err());
    let extra_field = r#"{"version":1,"source":"ai","ops":[{"op":"drop","clip":"c1","x":1}]}"#;
    assert!(parse(extra_field).is_err());
    let extra_root = r#"{"version":1,"source":"ai","ops":[],"secret":"x"}"#;
    assert!(parse(extra_root).is_err());
}

#[test]
fn aliases_follow_the_timeline_and_hide_ids() {
    let project = project();
    let aliases = Aliases::of(&project);
    assert_eq!(aliases.clip("c2"), Some(id(102)));
    assert_eq!(aliases.clip_alias(id(103)), Some("c3"));
    assert_eq!(aliases.segment("s1"), Some((id(101), id(200))));
    assert_eq!(aliases.clip("c4"), None);
}

#[test]
fn a_full_plan_becomes_one_group_that_does_everything() {
    let json = r#"{
      "version": 1, "source": "ai", "summary": "Hook first.",
      "ops": [
        {"op": "order", "clips": ["c3", "c1"]},
        {"op": "trim", "clip": "c1", "in_ms": 500, "out_ms": 3500, "reason": "silence"},
        {"op": "drop", "clip": "c2", "reason": "retake"},
        {"op": "split", "clip": "c3", "at_ms": 2000},
        {"op": "transition", "after": "c3", "kind": "crossfade", "duration_ms": 300},
        {"op": "emphasis", "words": [{"segment": "s1", "word_index": 0}]}
      ],
      "uncertain": [{"clip": "c1", "reason": "two takes"}]
    }"#;
    let plan = check(json).unwrap();
    let Command::Group { label, .. } = &plan.command else {
        panic!("not a group");
    };
    assert_eq!(label, AI_LABEL);
    assert_eq!(plan.uncertain, vec![(id(101), "two takes".to_string())]);
    assert!(plan.changes.iter().any(|c| c.kind == ChangeKind::Dropped
        && c.clip == id(102)
        && c.reason.as_deref() == Some("retake")));

    let mut project = project();
    plan.command.apply(&mut project).unwrap();
    let clips = &project.timeline.clips;
    assert_eq!(
        clips.iter().map(|c| c.id).collect::<Vec<_>>(),
        vec![id(103), id(501), id(101)]
    );
    assert_eq!(
        (clips[0].source_in_us, clips[0].source_out_us),
        (0, 2_000_000)
    );
    assert_eq!(
        (clips[2].source_in_us, clips[2].source_out_us),
        (500_000, 3_500_000)
    );
    let transition = &project.timeline.transitions[0];
    assert_eq!(
        (transition.kind, transition.duration_us),
        (TransitionKind::Crossfade, 300_000)
    );
    let first = &clips[2].transcript.as_ref().unwrap().segments[0].words[0];
    assert_eq!(first.emphasis, EmphasisState::User(true));
    assert_eq!(plan.duration_us, project.duration_us());
}

#[test]
fn a_cut_removes_a_stretch_inside_a_clip() {
    let json = r#"{"version":1,"source":"local","ops":[
        {"op":"trim","clip":"c2","in_ms":500,"out_ms":4000},
        {"op":"cut","clip":"c2","from_ms":1000,"to_ms":2500,"reason":"pause"},
        {"op":"cut","clip":"c2","from_ms":3000,"to_ms":4000}]}"#;
    let plan = check(json).unwrap();
    let mut project = project();
    plan.command.apply(&mut project).unwrap();
    let ranges: Vec<_> = project
        .timeline
        .clips
        .iter()
        .map(|c| (c.source_in_us, c.source_out_us))
        .collect();
    assert_eq!(
        ranges,
        vec![
            (0, 4_000_000),
            (500_000, 1_000_000),
            (2_500_000, 3_000_000),
            (0, 4_000_000)
        ]
    );
    let overlapping = r#"{"version":1,"source":"local","ops":[
        {"op":"cut","clip":"c1","from_ms":1000,"to_ms":2500},
        {"op":"cut","clip":"c1","from_ms":2000,"to_ms":3000}]}"#;
    assert!(check(overlapping).unwrap_err()[0].contains("overlap"));
    let whole = r#"{"version":1,"source":"local","ops":[
        {"op":"cut","clip":"c1","from_ms":0,"to_ms":4000}]}"#;
    assert!(check(whole).unwrap_err()[0].contains("drop it instead"));
}

#[test]
fn every_problem_is_reported_at_once() {
    let json = r#"{"version": 2, "source": "ai", "ops": [
        {"op": "drop", "clip": "c9"},
        {"op": "trim", "clip": "c1", "in_ms": 3000, "out_ms": 1000},
        {"op": "trim", "clip": "c2", "in_ms": 0, "out_ms": 20000},
        {"op": "split", "clip": "c3", "at_ms": 9000},
        {"op": "order", "clips": ["c1", "c1"]},
        {"op": "order", "clips": ["c2"]},
        {"op": "transition", "after": "c1", "kind": "spin", "duration_ms": 300},
        {"op": "emphasis", "words": [{"segment": "s1", "word_index": 7}]}
    ]}"#;
    let errors = check(json).unwrap_err();
    for expected in [
        "version must be 1",
        "unknown clip \"c9\"",
        "trim of c1",
        "past the end",
        "split of c3",
        "appears twice",
        "only one order",
        "unknown transition",
        "no word 7",
    ] {
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "{expected} missing from {errors:?}"
        );
    }
}

#[test]
fn a_plan_that_drops_everything_or_ends_on_a_transition_is_refused() {
    let all = r#"{"version":1,"source":"local","ops":[
        {"op":"drop","clip":"c1"},{"op":"drop","clip":"c2"},{"op":"drop","clip":"c3"}]}"#;
    assert!(check(all).is_err());
    let last = r#"{"version":1,"source":"local","ops":[
        {"op":"transition","after":"c3","kind":"crossfade","duration_ms":300}]}"#;
    assert!(check(last).unwrap_err()[0].contains("no clip follows c3"));
}

#[test]
fn the_shipped_schema_names_every_op() {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA_V1).unwrap();
    let ops: Vec<&str> = schema["properties"]["ops"]["items"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|variant| variant["properties"]["op"]["const"].as_str().unwrap())
        .collect();
    assert_eq!(
        ops,
        [
            "order",
            "trim",
            "drop",
            "split",
            "cut",
            "transition",
            "emphasis"
        ]
    );
}

fn op() -> impl Strategy<Value = PlanOp> {
    let clip = (0u8..5).prop_map(|n| format!("c{n}"));
    prop_oneof![
        proptest::collection::vec(clip.clone(), 0..5).prop_map(|clips| PlanOp::Order { clips }),
        (clip.clone(), -500i64..12_000, -500i64..12_000).prop_map(|(clip, in_ms, out_ms)| {
            PlanOp::Trim {
                clip,
                in_ms,
                out_ms,
                reason: None,
            }
        }),
        clip.clone()
            .prop_map(|clip| PlanOp::Drop { clip, reason: None }),
        (clip.clone(), -100i64..12_000).prop_map(|(clip, at_ms)| PlanOp::Split { clip, at_ms }),
        (clip.clone(), -100i64..5_000, 0i64..5_000).prop_map(|(clip, from_ms, to_ms)| {
            PlanOp::Cut {
                clip,
                from_ms,
                to_ms,
                reason: None,
            }
        }),
        (
            clip,
            prop_oneof![Just("crossfade"), Just("push"), Just("spin")],
            0i64..3000
        )
            .prop_map(|(after, kind, duration_ms)| PlanOp::Transition {
                after,
                kind: kind.into(),
                duration_ms,
            }),
        (0u8..3, 0usize..5).prop_map(|(segment, word_index)| PlanOp::Emphasis {
            words: vec![WordRef {
                segment: format!("s{segment}"),
                word_index,
            }],
        }),
    ]
}

proptest! {
    /// Whatever a planner sends, either the whole plan applies and the project stays valid and
    /// undoable, or nothing is handed over to apply.
    #[test]
    fn plans_apply_completely_or_not_at_all(ops in proptest::collection::vec(op(), 0..8)) {
        let original = project();
        let plan = EditPlan {
            version: 1,
            source: PlanSource::Ai,
            summary: String::new(),
            ops,
            uncertain: Vec::new(),
        };
        match validate(&plan, &original, &Aliases::of(&original), &mut ids()) {
            Ok(valid) => {
                let mut edited = original.clone();
                let inverse = valid.command.apply(&mut edited).unwrap();
                prop_assert!(edited.validate().is_ok());
                prop_assert_eq!(edited.duration_us(), valid.duration_us);
                inverse.apply(&mut edited).unwrap();
                prop_assert_eq!(edited, original);
            }
            Err(errors) => prop_assert!(!errors.is_empty()),
        }
    }
}
