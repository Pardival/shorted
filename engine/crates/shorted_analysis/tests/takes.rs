//! Duplicate takes (T-0804): the fixture with two takes keeps the second.

#![allow(clippy::unwrap_used)]

use shorted_ai::plan::{Aliases, PlanOp};
use shorted_analysis::frames::FrameConfig;
use shorted_analysis::local::{REASON_DUPLICATE, add_duplicate_takes, analyse_clip, local_plan};
use shorted_analysis::speech::SpeechConfig;
use shorted_core::model::*;
use uuid::Uuid;

fn sentence(id: u128, text: &str, start_us: i64) -> Segment {
    Segment {
        id: Uuid::from_u128(id),
        hidden: false,
        words: text
            .split(' ')
            .enumerate()
            .map(|(index, word)| Word {
                text: word.into(),
                original_text: word.into(),
                start_us: start_us + index as i64 * 300_000,
                end_us: start_us + index as i64 * 300_000 + 250_000,
                confidence: 0.9,
                emphasis: EmphasisState::Auto(false),
                deleted: false,
            })
            .collect(),
    }
}

fn clip(id: u128, length_us: i64, segments: Vec<Segment>) -> ClipItem {
    let project =
        Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap();
    ClipItem {
        id: Uuid::from_u128(id),
        source_in_us: 0,
        source_out_us: length_us,
        speed: 1.0,
        image_motion: None,
        transcript: Some(Transcript {
            language: "en".into(),
            segments,
        }),
        ..project.timeline.clips[0].clone()
    }
}

fn plan_for(clips: Vec<ClipItem>) -> Vec<PlanOp> {
    let mut project = Project::new(Uuid::nil(), "t".into(), "2026-11-02T10:00:00Z".into());
    project.timeline.clips = clips;
    let mut analyses: Vec<_> = project
        .timeline
        .clips
        .iter()
        .map(|clip| {
            analyse_clip(
                clip,
                None,
                &[],
                200_000,
                &SpeechConfig::default(),
                &FrameConfig::default(),
            )
        })
        .collect();
    add_duplicate_takes(&project, &mut analyses);
    local_plan(&project, &analyses, &Aliases::of(&project)).ops
}

const TAKE_ONE: &str = "So today I want to show you my coffee setup";
const TAKE_TWO: &str = "so today I wanna show you my coffee set-up";

#[test]
fn two_takes_in_two_clips_keep_the_second() {
    let ops = plan_for(vec![
        clip(1, 3_500_000, vec![sentence(10, TAKE_ONE, 300_000)]),
        clip(2, 3_500_000, vec![sentence(20, TAKE_TWO, 300_000)]),
    ]);
    assert_eq!(
        ops,
        vec![PlanOp::Drop {
            clip: "c1".into(),
            reason: Some(REASON_DUPLICATE.into()),
        }]
    );
}

#[test]
fn a_retake_inside_a_clip_cuts_only_the_first_try() {
    let ops = plan_for(vec![clip(
        1,
        9_000_000,
        vec![
            sentence(10, TAKE_ONE, 300_000),
            sentence(11, TAKE_TWO, 3_600_000),
            sentence(12, "and here is the grinder I use", 6_800_000),
        ],
    )]);
    // Without a sound track the short silence before it stays: only the first try goes.
    let [
        PlanOp::Cut {
            from_ms,
            to_ms,
            reason,
            ..
        },
    ] = ops.as_slice()
    else {
        panic!("expected one cut, got {ops:?}");
    };
    assert_eq!(reason.as_deref(), Some(REASON_DUPLICATE));
    assert!(*from_ms <= 300, "{from_ms}");
    assert!(*to_ms >= 3_250 && *to_ms < 3_600, "{to_ms}");
}

#[test]
fn different_sentences_and_short_ones_stay() {
    let ops = plan_for(vec![
        clip(1, 3_500_000, vec![sentence(10, TAKE_ONE, 300_000)]),
        clip(
            2,
            3_500_000,
            vec![sentence(20, "then we grind the beans", 300_000)],
        ),
        clip(3, 2_000_000, vec![sentence(30, "okay so", 300_000)]),
        clip(4, 2_000_000, vec![sentence(40, "okay so", 300_000)]),
    ]);
    assert!(ops.is_empty(), "{ops:?}");
}

#[test]
fn an_unfinished_later_take_does_not_replace_a_complete_one() {
    let ops = plan_for(vec![
        clip(1, 3_500_000, vec![sentence(10, TAKE_ONE, 300_000)]),
        clip(2, 2_000_000, vec![sentence(20, "So today I want", 300_000)]),
    ]);
    assert!(ops.is_empty(), "{ops:?}");
}
