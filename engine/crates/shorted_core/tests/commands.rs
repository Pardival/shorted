//! Command, undo and gesture behaviour (T-0102).

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use shorted_core::command::*;
use shorted_core::editor::*;
use shorted_core::error::ModelError;
use shorted_core::model::*;
use uuid::Uuid;

fn id(value: u128) -> Id {
    Uuid::from_u128(value)
}

fn project() -> Project {
    let json = include_str!("../../../fixtures/projects/v1_full.json");
    Project::from_json(json).unwrap()
}

const CLIPS: [u128; 2] = [20, 21];
const OVERLAYS: [u128; 5] = [50, 51, 52, 53, 54];

fn any_clip() -> impl Strategy<Value = Id> {
    prop::sample::select(CLIPS.map(id).to_vec())
}

fn any_overlay() -> impl Strategy<Value = Id> {
    prop::sample::select(OVERLAYS.map(id).to_vec())
}

fn any_audio() -> impl Strategy<Value = Id> {
    prop::sample::select(vec![id(40), id(41), id(999)])
}

fn transform() -> impl Strategy<Value = Transform> {
    (-500.0f32..1500.0, 0.1f32..4.0, 0.0f32..=1.0).prop_map(|(x, scale, opacity)| Transform {
        position: Vec2 { x, y: x / 2.0 },
        scale,
        rotation: x,
        opacity,
        flip: x > 0.0,
    })
}

fn command() -> impl Strategy<Value = Command> {
    let transcript_owner = prop_oneof![
        Just(TranscriptOwner::Clip(id(20))),
        Just(TranscriptOwner::Audio(id(40))),
        Just(TranscriptOwner::Audio(id(41))),
    ];
    let property = prop::sample::select(vec![
        Prop::X,
        Prop::Y,
        Prop::Scale,
        Prop::Rotation,
        Prop::Opacity,
    ]);
    prop_oneof![
        "[a-z ]{0,12}".prop_map(|title| Command::Rename { title }),
        prop::option::of("[a-z]{2}").prop_map(|language| Command::SetLanguage { language }),
        prop::sample::select(vec![24u32, 30, 60]).prop_map(|fps| Command::SetCanvasFps { fps }),
        Just(Command::RemoveMedia { id: id(13) }),
        (any_clip(), 0usize..3).prop_map(|(id, to)| Command::MoveClip { id, to }),
        (any_clip(), 0i64..2_000_000, 1i64..4_000_000).prop_map(
            |(id, source_in_us, source_out_us)| {
                Command::TrimClip {
                    id,
                    source_in_us,
                    source_out_us,
                }
            }
        ),
        (any_clip(), 1i64..3_999_999).prop_map(|(clip, at_source_us)| Command::SplitClip {
            id: clip,
            at_source_us,
            new_id: id(777),
        }),
        any_clip().prop_map(|id| Command::DeleteClip { id }),
        (any_clip(), 0.3f32..2.5).prop_map(|(id, speed)| Command::SetSpeed { id, speed }),
        (100_000i64..1_500_000).prop_map(|duration_us| Command::SetTransitionForAll {
            kind: TransitionKind::Glitch,
            duration_us,
        }),
        Just(Command::SetTransition {
            left: id(20),
            right: id(21),
            kind: TransitionKind::Cut,
            duration_us: 0,
        }),
        any_audio().prop_map(|id| Command::DeleteAudio { id }),
        (any_audio(), 0i64..3_000_000).prop_map(|(id, start_us)| Command::MoveAudio {
            id,
            start_us,
            track: AudioTrackRef::VoiceOver,
        }),
        (any_audio(), 0.0f32..5.0).prop_map(|(id, volume)| Command::SetVolume { id, volume }),
        Just(Command::AddAudioTrack {
            track: AudioTrack {
                id: id(31),
                name: "New".into(),
                flags: TrackFlags::default()
            },
        }),
        Just(Command::RemoveAudioTrack { id: id(30) }),
        any::<bool>().prop_map(|muted| Command::SetTrackFlags {
            track: TrackKind::Widgets,
            flags: TrackFlags {
                muted,
                locked: !muted,
                hidden: false
            },
        }),
        (transcript_owner.clone(), 0usize..3, "[a-z]{1,6}").prop_map(|(owner, word, text)| {
            Command::EditWord {
                owner,
                segment: id(900),
                word,
                text,
            }
        }),
        (transcript_owner.clone(), any::<bool>()).prop_map(|(owner, hidden)| {
            Command::HideSegment {
                owner,
                segment: id(900),
                hidden,
            }
        }),
        (transcript_owner.clone(), 0usize..3).prop_map(|(owner, word)| {
            Command::ToggleEmphasis {
                owner,
                segment: id(900),
                word,
            }
        }),
        (transcript_owner, prop::collection::vec(0usize..3, 0..3)).prop_map(|(owner, words)| {
            Command::DeleteWords {
                owner,
                segment: id(900),
                words,
                deleted: true,
            }
        }),
        (any_overlay(), transform())
            .prop_map(|(id, base)| Command::UpdateOverlayTransform { id, base }),
        (any_overlay(), 0i64..1_000_000, 0i64..3_000_000).prop_map(|(id, start_us, end_us)| {
            Command::SetOverlayTiming {
                id,
                start_us,
                end_us,
            }
        }),
        (
            any_overlay(),
            property.clone(),
            0i64..1_000_000,
            -5.0f32..5.0
        )
            .prop_map(|(id, property, t_us, value)| Command::SetKeyframe {
                id,
                property,
                key: Key {
                    t_us,
                    value,
                    easing: Easing::Linear
                },
            }),
        (any_overlay(), property, 0i64..1_000_000)
            .prop_map(|(id, property, t_us)| { Command::RemoveKeyframe { id, property, t_us } }),
        (any_overlay(), 0usize..6).prop_map(|(id, to)| Command::ReorderOverlay { id, to }),
        any_overlay().prop_map(|id| Command::DeleteOverlay { id }),
        any_overlay().prop_map(|id| Command::SetMotionPath { id, path: None }),
        any_overlay().prop_map(|id| Command::SetPresetAnim {
            id,
            slot: AnimSlot::Out,
            anim: None
        }),
    ]
}

fn group() -> impl Strategy<Value = Command> {
    prop_oneof![
        3 => command(),
        1 => prop::collection::vec(command(), 0..5)
            .prop_map(|commands| Command::Group { label: "group".into(), commands }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn apply_then_inverse_restores_the_project(command in group()) {
        let original = project();
        let mut edited = original.clone();
        match command.apply(&mut edited) {
            Ok(inverse) => {
                let redo = inverse.apply(&mut edited).unwrap();
                prop_assert_eq!(&edited, &original);
                // The inverse of the inverse redoes the edit.
                let mut again = original.clone();
                command.apply(&mut again).unwrap();
                redo.apply(&mut edited).unwrap();
                prop_assert_eq!(edited, again);
            }
            // A refused command leaves the project untouched.
            Err(_) => prop_assert_eq!(edited, original),
        }
    }

    #[test]
    fn undo_all_then_redo_all_walks_the_same_states(commands in prop::collection::vec(group(), 1..12)) {
        let original = project();
        let mut editor = Editor::new(original.clone());
        for command in &commands {
            let _ = editor.execute(command);
        }
        let last = editor.project().clone();
        while editor.undo().unwrap().is_some() {}
        prop_assert_eq!(editor.project(), &original);
        while editor.redo().unwrap().is_some() {}
        prop_assert_eq!(editor.project(), &last);
    }

    #[test]
    fn commands_survive_the_journal_format(command in group()) {
        let json = serde_json::to_string(&command).unwrap();
        prop_assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
    }
}

fn drag_to(x: f32) -> Command {
    Command::UpdateOverlayTransform {
        id: id(50),
        base: Transform {
            position: Vec2 { x, y: 0.0 },
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            flip: false,
        },
    }
}

#[test]
fn a_drag_is_many_previews_and_one_undo_step() {
    let original = project();
    let mut editor = Editor::new(original.clone());
    for x in 0..120 {
        let event = editor.preview(&drag_to(x as f32)).unwrap();
        assert!(event.transient);
        assert_eq!(event.changed, [ChangedArea::Overlays]);
    }
    assert!(!editor.can_undo());
    let event = editor.execute(&drag_to(119.0)).unwrap();
    assert!(!event.transient);

    assert!(editor.undo().unwrap().is_some());
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
    assert!(editor.redo().unwrap().is_some());
    assert_eq!(editor.project().timeline.overlays[0].base.position.x, 119.0);
}

#[test]
fn a_cancelled_gesture_leaves_no_trace() {
    let original = project();
    let mut editor = Editor::new(original.clone());
    editor.preview(&drag_to(5.0)).unwrap();
    editor.cancel_gesture().unwrap();
    assert_eq!(editor.project(), &original);
    assert!(!editor.can_undo());
    assert_eq!(editor.cancel_gesture().unwrap_err(), ModelError::NoGesture);
}

#[test]
fn undo_during_a_gesture_drops_the_gesture_first() {
    let mut editor = Editor::new(project());
    editor
        .execute(&Command::Rename { title: "A".into() })
        .unwrap();
    editor.preview(&drag_to(5.0)).unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.project(), &project());
}

#[test]
fn history_keeps_at_least_200_steps() {
    let original = project();
    let mut editor = Editor::new(original.clone());
    for step in 0..250 {
        editor
            .execute(&Command::Rename {
                title: step.to_string(),
            })
            .unwrap();
    }
    let mut steps = 0;
    while editor.undo().unwrap().is_some() {
        steps += 1;
    }
    assert_eq!(steps, 250);
    assert_eq!(editor.project(), &original);
}

#[test]
fn history_is_bounded() {
    let mut editor = Editor::new(project());
    for step in 0..=MAX_UNDO_STEPS {
        editor
            .execute(&Command::Rename {
                title: step.to_string(),
            })
            .unwrap();
    }
    let mut steps = 0;
    while editor.undo().unwrap().is_some() {
        steps += 1;
    }
    assert_eq!(steps, MAX_UNDO_STEPS);
}

#[test]
fn a_new_edit_clears_redo() {
    let mut editor = Editor::new(project());
    editor
        .execute(&Command::Rename { title: "A".into() })
        .unwrap();
    editor.undo().unwrap();
    assert!(editor.can_redo());
    editor
        .execute(&Command::Rename { title: "B".into() })
        .unwrap();
    assert!(!editor.can_redo());
}

#[test]
fn refused_commands_do_not_touch_history_or_revision() {
    let mut editor = Editor::new(project());
    let refused = Command::SetSpeed {
        id: id(20),
        speed: 10.0,
    };
    assert!(editor.execute(&refused).is_err());
    assert_eq!(editor.revision(), 0);
    assert!(!editor.can_undo());
}

#[test]
fn a_failing_group_changes_nothing() {
    let mut project = project();
    let before = project.clone();
    let group = Command::Group {
        label: "smart arrange".into(),
        commands: vec![
            Command::Rename { title: "A".into() },
            Command::DeleteClip { id: id(999) },
        ],
    };
    assert_eq!(
        group.apply(&mut project).unwrap_err(),
        ModelError::UnknownId(id(999))
    );
    assert_eq!(project, before);
}

#[test]
fn split_keeps_total_source_and_moves_the_outgoing_transition() {
    let mut project = project();
    Command::SplitClip {
        id: id(20),
        at_source_us: 1_000_000,
        new_id: id(77),
    }
    .apply(&mut project)
    .unwrap();
    let clips = &project.timeline.clips;
    assert_eq!(clips[0].source_out_us, 1_000_000);
    assert_eq!(clips[1].source_in_us, 1_000_000);
    assert_eq!(clips[1].source_out_us, 4_000_000);
    assert_eq!(project.timeline.transitions[0].left, id(77));
}

#[test]
fn moving_a_clip_drops_the_transition_it_broke() {
    let mut project = project();
    Command::MoveClip { id: id(20), to: 1 }
        .apply(&mut project)
        .unwrap();
    assert!(project.timeline.transitions.is_empty());
}

#[test]
fn media_in_use_cannot_be_removed() {
    let mut project = project();
    let error = Command::RemoveMedia { id: id(10) }
        .apply(&mut project)
        .unwrap_err();
    assert_eq!(error, ModelError::UnknownId(id(10)));
}

#[test]
fn removing_a_track_removes_its_items_and_undo_brings_them_back() {
    let mut project = project();
    let original = project.clone();
    let inverse = Command::RemoveAudioTrack { id: id(30) }
        .apply(&mut project)
        .unwrap();
    assert_eq!(project.timeline.audio.len(), 1);
    inverse.apply(&mut project).unwrap();
    assert_eq!(project, original);
}

#[test]
fn deleting_a_captioned_audio_item_removes_its_caption_source() {
    let mut project = project();
    Command::DeleteAudio { id: id(40) }
        .apply(&mut project)
        .unwrap();
    assert_eq!(project.captions.sources, [CaptionSource::OriginalSound]);
}

#[test]
fn events_name_what_changed() {
    let mut editor = Editor::new(project());
    let event = editor.execute(&Command::DeleteClip { id: id(21) }).unwrap();
    assert_eq!(event.revision, 1);
    assert_eq!(
        event.changed,
        [ChangedArea::Clips, ChangedArea::Transitions]
    );
}

#[test]
fn toggling_emphasis_becomes_a_user_choice() {
    let mut project = project();
    Command::ToggleEmphasis {
        owner: TranscriptOwner::Clip(id(20)),
        segment: id(900),
        word: 0,
    }
    .apply(&mut project)
    .unwrap();
    let word = &project.timeline.clips[0]
        .transcript
        .as_ref()
        .unwrap()
        .segments[0]
        .words[0];
    assert_eq!(word.emphasis, EmphasisState::User(false));
}
