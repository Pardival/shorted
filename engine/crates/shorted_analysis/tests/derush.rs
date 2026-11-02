//! The derush fixture set (T-0801): at least 90 % of the labelled dead time goes, and no
//! labelled speech is ever cut.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use serde::Deserialize;
use shorted_ai::plan::{Aliases, PlanOp};
use shorted_analysis::frames::FrameConfig;
use shorted_analysis::local::{Sound, analyse_clip, local_plan};
use shorted_analysis::speech::SpeechConfig;
use shorted_core::model::{ClipItem, Project};
use uuid::Uuid;

const RATE: u32 = 16_000;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    duration_ms: i64,
    room: f32,
    speech: Vec<(i64, i64)>,
    dead: Vec<(i64, i64)>,
}

/// A repeatable noise source, so the set gives the same answer everywhere.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    }
}

/// Room noise everywhere, and syllables of louder noise where the label says speech.
fn sound(fixture: &Fixture) -> Vec<f32> {
    let mut noise = Noise(fixture.name.len() as u64 * 7919);
    let samples = (fixture.duration_ms * i64::from(RATE) / 1000) as usize;
    (0..samples)
        .map(|index| {
            let ms = index as i64 * 1000 / i64::from(RATE);
            let speaking = fixture.speech.iter().any(|(a, b)| ms >= *a && ms < *b);
            let room = fixture.room * noise.next();
            if speaking {
                // Five syllables a second, never quite silent between them.
                let phase = (ms % 200) as f32 / 200.0;
                let syllable = 0.35 + 0.65 * (phase * std::f32::consts::PI).sin();
                room + 0.25 * syllable * noise.next()
            } else {
                room
            }
        })
        .collect()
}

fn fixtures() -> Vec<Fixture> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/derush");
    let mut found: Vec<Fixture> = fs::read_dir(folder)
        .unwrap()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .map(|entry| serde_json::from_str(&fs::read_to_string(entry.path()).unwrap()).unwrap())
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// What a plan removes from the clip, in source milliseconds.
fn removed(ops: &[PlanOp], duration_ms: i64) -> Vec<(i64, i64)> {
    let mut gone = Vec::new();
    for op in ops {
        match op {
            PlanOp::Trim { in_ms, out_ms, .. } => {
                gone.push((0, *in_ms));
                gone.push((*out_ms, duration_ms));
            }
            PlanOp::Cut { from_ms, to_ms, .. } => gone.push((*from_ms, *to_ms)),
            PlanOp::Drop { .. } => gone.push((0, duration_ms)),
            _ => {}
        }
    }
    gone.retain(|(a, b)| b > a);
    gone
}

fn overlap(a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.1.min(b.1) - a.0.max(b.0)).max(0)
}

fn clip(duration_ms: i64) -> ClipItem {
    let project =
        Project::from_json(include_str!("../../../fixtures/projects/v1_full.json")).unwrap();
    ClipItem {
        id: Uuid::from_u128(1),
        source_in_us: 0,
        source_out_us: duration_ms * 1000,
        speed: 1.0,
        image_motion: None,
        transcript: None,
        ..project.timeline.clips[0].clone()
    }
}

#[test]
fn the_fixture_set_loses_its_dead_time_and_keeps_every_word() {
    let mut labelled = 0;
    let mut cleared = 0;
    for fixture in fixtures() {
        let mut project = Project::new(Uuid::nil(), "t".into(), "2026-11-02T10:00:00Z".into());
        let item = clip(fixture.duration_ms);
        project.timeline.clips.push(item.clone());
        let mono = sound(&fixture);
        let heard = Sound {
            mono: &mono,
            rate: RATE,
        }
        .speech(&SpeechConfig::default());
        let analysis = analyse_clip(
            &item,
            Some(&heard),
            &[],
            200_000,
            &SpeechConfig::default(),
            &FrameConfig::default(),
        );
        let plan = local_plan(&project, &[analysis], &Aliases::of(&project));
        let gone = removed(&plan.ops, fixture.duration_ms);

        for speech in &fixture.speech {
            for range in &gone {
                assert_eq!(
                    overlap(*speech, *range),
                    0,
                    "{}: speech {speech:?} is cut by {range:?}",
                    fixture.name
                );
            }
        }
        for dead in &fixture.dead {
            labelled += 1;
            let covered: i64 = gone.iter().map(|range| overlap(*dead, *range)).sum();
            if covered * 10 >= (dead.1 - dead.0) * 8 {
                cleared += 1;
            } else {
                eprintln!("{}: dead time {dead:?} mostly kept", fixture.name);
            }
        }
    }
    assert!(labelled >= 10, "the set is too small");
    assert!(
        cleared * 10 >= labelled * 9,
        "{cleared} of {labelled} dead-time ranges removed"
    );
}
