//! Round trips for every entity and rejection of invalid documents (T-0101).

#![allow(clippy::unwrap_used)]

use std::path::Path;

use shorted_core::error::ModelError;
use shorted_core::model::*;
use shorted_core::validate::{MAX_AUDIO_TRACKS, MAX_CLIPS, MAX_DOCUMENT_BYTES, MAX_OVERLAYS};
use uuid::Uuid;

type Change = fn(&mut Project);

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn word(start_us: i64, emphasis: EmphasisState) -> Word {
    Word {
        text: "éditer".into(),
        original_text: "editer".into(),
        start_us,
        end_us: start_us + 400_000,
        confidence: 0.92,
        emphasis,
        deleted: false,
    }
}

fn transcript() -> Transcript {
    Transcript {
        language: "fr".into(),
        segments: vec![Segment {
            id: id(900),
            words: vec![
                word(0, EmphasisState::Auto(true)),
                word(500_000, EmphasisState::User(false)),
            ],
            hidden: false,
        }],
    }
}

fn media(value: u128, kind: MediaKind, location: MediaLocation) -> MediaRef {
    MediaRef {
        id: id(value),
        kind,
        location,
        content_hash: "abc123".into(),
        probe: MediaProbe {
            duration_us: 10_000_000,
            width: 1920,
            height: 1080,
            rotation: 90,
            fps: 59.94,
            has_audio: true,
            codec: "h264".into(),
            color: ColorRange::Hdr,
        },
        captured_at: Some("2026-11-01T10:00:00Z".into()),
    }
}

fn clip(value: u128, media_id: u128) -> ClipItem {
    ClipItem {
        id: id(value),
        media_id: id(media_id),
        source_in_us: 0,
        source_out_us: 4_000_000,
        speed: 1.5,
        crop: Crop {
            center: Vec2 { x: 0.5, y: 0.5 },
            scale: 1.2,
            rotation: 0.0,
        },
        color: ColorAdjust {
            brightness: 0.1,
            contrast: 1.0,
            saturation: 1.0,
        },
        image_motion: None,
        original_audio: ClipAudio {
            volume: 1.0,
            muted: false,
            fade_in_us: 0,
            fade_out_us: 200_000,
            transcribe: true,
        },
        transcript: Some(transcript()),
    }
}

fn overlay(value: u128, kind: OverlayKind) -> OverlayItem {
    OverlayItem {
        id: id(value),
        kind,
        start_us: 0,
        end_us: 2_000_000,
        base: Transform {
            position: Vec2 { x: 540.0, y: 960.0 },
            scale: 1.0,
            rotation: 12.5,
            opacity: 0.8,
            flip: true,
        },
        keyframes: vec![AnimatedProperty {
            property: Prop::Opacity,
            keys: vec![
                Key {
                    t_us: 0,
                    value: 0.0,
                    easing: Easing::EaseInOut,
                },
                Key {
                    t_us: 500_000,
                    value: 1.0,
                    easing: Easing::Spring,
                },
            ],
        }],
        path: Some(MotionPath {
            samples: vec![
                PathSample {
                    t_us: 0,
                    x: 1.0,
                    y: 2.0,
                },
                PathSample {
                    t_us: 16_000,
                    x: 3.0,
                    y: 4.0,
                },
            ],
            smoothing: 0.4,
            orient_to_path: true,
        }),
        anim_in: Some(PresetAnim {
            kind: PresetKind::Pop,
            duration_us: 300_000,
        }),
        anim_loop: Some(PresetAnim {
            kind: PresetKind::Wiggle,
            duration_us: 1_000_000,
        }),
        anim_out: None,
        locked: false,
        hidden: false,
    }
}

/// Uses every entity, and the variants carrying data.
fn full_project() -> Project {
    let mut project = Project::new(id(1), "Ma journée".into(), "2026-11-01T10:00:00Z".into());
    project.language = Some("fr".into());
    project.media = vec![
        media(
            10,
            MediaKind::Video,
            MediaLocation::ProjectCopy {
                rel_path: "media/a.mov".into(),
            },
        ),
        media(
            11,
            MediaKind::Image,
            MediaLocation::External {
                abs_path: "/tmp/b.png".into(),
                bookmark: Some("xyz".into()),
            },
        ),
        media(
            12,
            MediaKind::Audio,
            MediaLocation::External {
                abs_path: "/tmp/c.m4a".into(),
                bookmark: None,
            },
        ),
        media(
            13,
            MediaKind::AnimatedImage,
            MediaLocation::ProjectCopy {
                rel_path: "media/d.gif".into(),
            },
        ),
    ];
    let mut image = clip(21, 11);
    image.image_motion = Some(KenBurns {
        start_scale: 1.0,
        end_scale: 1.2,
        start_center: Vec2 { x: 0.5, y: 0.5 },
        end_center: Vec2 { x: 0.4, y: 0.6 },
    });
    project.timeline.clips = vec![clip(20, 10), image];
    project.timeline.transitions = vec![Transition {
        left: id(20),
        right: id(21),
        kind: TransitionKind::WhipPan,
        duration_us: 300_000,
    }];
    let track = AudioTrack {
        id: id(30),
        name: "Musique".into(),
        flags: TrackFlags {
            muted: true,
            locked: false,
            hidden: false,
        },
    };
    project.timeline.audio_tracks = vec![track];
    let voice = AudioItem {
        id: id(40),
        media_id: id(12),
        role: AudioRole::VoiceOver,
        track: AudioTrackRef::VoiceOver,
        start_us: 0,
        source_in_us: 0,
        source_out_us: 1_000_000,
        volume: 1.0,
        fade_in_us: 0,
        fade_out_us: 0,
        loop_: false,
        ducking: None,
        transcript: Some(transcript()),
    };
    let music = AudioItem {
        id: id(41),
        role: AudioRole::Music,
        track: AudioTrackRef::User(id(30)),
        loop_: true,
        ducking: Some(Ducking { strength: 0.6 }),
        transcript: None,
        ..voice.clone()
    };
    project.timeline.audio = vec![voice, music];
    project.timeline.overlays = vec![
        overlay(50, OverlayKind::Image { asset: id(11) }),
        overlay(51, OverlayKind::AnimatedImage { asset: id(13) }),
        overlay(
            52,
            OverlayKind::Video {
                media: id(10),
                source_in_us: 0,
            },
        ),
        overlay(
            53,
            OverlayKind::Text {
                content: "Regarde".into(),
                style: TextStyle::default(),
            },
        ),
        overlay(
            54,
            OverlayKind::Shape {
                shape: ShapeKind::Arrow,
                fill: "#FF0000FF".into(),
            },
        ),
    ];
    project.captions.sources = vec![
        CaptionSource::OriginalSound,
        CaptionSource::AudioItem(id(40)),
    ];
    project.captions.grouping = Grouping::UpTo(3);
    project
}

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/projects")
}

#[test]
fn full_project_is_valid_and_round_trips() {
    let project = full_project();
    project.validate().unwrap();
    assert_eq!(
        Project::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
}

#[test]
fn fixture_projects_load() {
    let mut count = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let json = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        Project::from_json(&json).unwrap();
        count += 1;
    }
    assert!(count >= 2);
}

#[test]
fn fixtures_match_the_current_model() {
    let path = fixtures().join("v1_full.json");
    let expected = serde_json::to_string_pretty(&full_project()).unwrap();
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &expected).unwrap();
        let empty = Project::new(id(2), "Empty".into(), "2026-11-01T10:00:00Z".into());
        std::fs::write(
            fixtures().join("v1_empty.json"),
            serde_json::to_string_pretty(&empty).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
}

#[test]
fn duration_applies_speed_and_transitions() {
    let project = full_project();
    // Two clips of 4 s at 1.5×, minus a 0.3 s transition.
    assert_eq!(project.duration_us(), 2_666_667 * 2 - 300_000);
}

fn rejected(change: impl FnOnce(&mut Project)) -> ModelError {
    let mut project = full_project();
    change(&mut project);
    project.validate().unwrap_err()
}

#[test]
fn limits_are_enforced() {
    let error = rejected(|p| {
        let item = p.timeline.clips[0].clone();
        p.timeline.clips = (0..=MAX_CLIPS as u128)
            .map(|i| ClipItem {
                id: id(1000 + i),
                ..item.clone()
            })
            .collect();
        p.timeline.transitions.clear();
    });
    assert!(matches!(error, ModelError::TooMany { what: "clips", .. }));

    let error = rejected(|p| {
        let item = p.timeline.overlays[0].clone();
        p.timeline.overlays = (0..=MAX_OVERLAYS as u128)
            .map(|i| OverlayItem {
                id: id(1000 + i),
                ..item.clone()
            })
            .collect();
    });
    assert!(matches!(
        error,
        ModelError::TooMany {
            what: "overlays",
            ..
        }
    ));

    let error = rejected(|p| {
        let item = p.timeline.audio_tracks[0].clone();
        p.timeline.audio_tracks = (0..=MAX_AUDIO_TRACKS as u128)
            .map(|i| AudioTrack {
                id: id(1000 + i),
                ..item.clone()
            })
            .collect();
    });
    assert!(matches!(
        error,
        ModelError::TooMany {
            what: "audio tracks",
            ..
        }
    ));
}

#[test]
fn invalid_values_are_rejected() {
    let cases: Vec<(&str, Change)> = vec![
        ("clip speed", |p| p.timeline.clips[0].speed = 3.0),
        ("clip speed", |p| p.timeline.clips[0].speed = f32::NAN),
        ("clip range", |p| p.timeline.clips[0].source_out_us = 0),
        ("clip start", |p| p.timeline.clips[0].source_in_us = -1),
        ("clip end", |p| p.timeline.clips[0].source_out_us = i64::MAX),
        ("clip media kind", |p| p.timeline.clips[0].media_id = id(12)),
        ("image motion", |p| {
            p.timeline.clips[0].image_motion = p.timeline.clips[1].image_motion
        }),
        ("transition duration", |p| {
            p.timeline.transitions[0].duration_us = 2_000_000
        }),
        ("transition clips", |p| p.timeline.clips.swap(0, 1)),
        ("audio overlap", |p| {
            p.timeline.audio[1].track = AudioTrackRef::VoiceOver
        }),
        ("ducking", |p| {
            p.timeline.audio[0].ducking = Some(Ducking { strength: 1.0 })
        }),
        ("overlay range", |p| p.timeline.overlays[0].end_us = 0),
        ("overlay opacity", |p| {
            p.timeline.overlays[0].base.opacity = 2.0
        }),
        ("keyframe order", |p| {
            p.timeline.overlays[0].keyframes[0].keys[1].t_us = 0
        }),
        ("path order", |p| {
            p.timeline.overlays[0].path.as_mut().unwrap().samples[1].t_us = 0
        }),
        ("colour", |p| p.captions.emphasis_color = "red".into()),
        ("canvas", |p| p.canvas.width = 1920),
        ("word range", |p| {
            p.timeline.clips[0].transcript.as_mut().unwrap().segments[0].words[0].end_us = -1
        }),
    ];
    for (expected, change) in cases {
        match rejected(change) {
            ModelError::Invalid { what } if what == expected => {}
            ModelError::Invalid { what: "word end" } if expected == "word range" => {}
            other => panic!("expected {expected}, got {other:?}"),
        }
    }
}

#[test]
fn references_and_duplicates_are_rejected() {
    assert_eq!(
        rejected(|p| p.timeline.clips[0].media_id = id(999)),
        ModelError::UnknownId(id(999))
    );
    assert_eq!(
        rejected(|p| p.timeline.clips[1].id = id(20)),
        ModelError::DuplicateId(id(20))
    );
    assert_eq!(
        rejected(|p| p.captions.sources.push(CaptionSource::AudioItem(id(999)))),
        ModelError::UnknownId(id(999))
    );
    assert_eq!(
        rejected(|p| p.timeline.overlays[0].kind = OverlayKind::Video {
            media: id(999),
            source_in_us: 0
        }),
        ModelError::UnknownId(id(999))
    );
}

#[test]
fn schema_versions_are_typed() {
    assert_eq!(
        rejected(|p| p.schema_version = 2),
        ModelError::NewerSchema { found: 2 }
    );
    assert_eq!(
        rejected(|p| p.schema_version = 0),
        ModelError::OlderSchema { found: 0 }
    );
}

#[test]
fn bad_documents_return_errors_without_panicking() {
    for json in ["", "{", "null", "[]", "{\"schema_version\":1}", "\"x\""] {
        assert!(
            matches!(Project::from_json(json), Err(ModelError::Malformed(_))),
            "{json}"
        );
    }
    let deep = "[".repeat(100_000);
    assert!(matches!(
        Project::from_json(&deep),
        Err(ModelError::Malformed(_))
    ));
    let huge = " ".repeat(MAX_DOCUMENT_BYTES + 1);
    assert!(matches!(
        Project::from_json(&huge),
        Err(ModelError::DocumentTooLarge { .. })
    ));
}

#[test]
fn messages_do_not_echo_user_text_for_valid_shape_errors() {
    let error = rejected(|p| p.captions.emphasis_color = "secret".into());
    assert!(!error.to_string().contains("secret"));
}
