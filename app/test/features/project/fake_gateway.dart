import 'dart:async';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:shorted/bridge/api/audio.dart';
import 'package:shorted/bridge/api/look.dart';
import 'package:shorted/bridge/api/media.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/bridge/runtime.dart';
import 'package:shorted/features/project/project_gateway.dart';

PlatformInt64 big(int value) => PlatformInt64Util.from(value);

class FakeProject {
  FakeProject(this.id, this.title);

  final String id;
  String title;
  final media = <MediaItemView>[];
  final clips = <String>[];
  final stills = <String>{};
  final audioTracks = <AudioTrackView>[];
  final titles = <String>[];
  final redone = <String>[];
}

/// In-memory stand-in for the engine.
class FakeGateway implements ProjectGateway {
  final projects = <FakeProject>[];
  final trashed = <FakeProject>[];
  final saves = StreamController<SaveStateView>.broadcast();
  ErrorCode? failOpenWith;
  FakeProject? opened;
  var _next = 0;

  ProjectSummary _summary(FakeProject project) => ProjectSummary(
    id: project.id,
    title: project.title,
    durationUs: big(0),
    editedAtMs: big(1793000000000),
  );

  FakeProject _find(String id) => projects.firstWhere((p) => p.id == id);

  @override
  Future<List<ProjectSummary>> list() async => projects.map(_summary).toList();

  @override
  Future<List<ProjectSummary>> trash() async => trashed.map(_summary).toList();

  @override
  Future<String> create(String title) async {
    final project = FakeProject('p${_next++}', title);
    projects.add(project);
    opened = project;
    return project.id;
  }

  @override
  Future<void> open(String id) async {
    final code = failOpenWith;
    if (code != null) throw EngineFailure(code: code);
    opened = _find(id);
  }

  @override
  Future<void> close() async => opened = null;

  @override
  Future<String> duplicate(String id, String title) async {
    final project = FakeProject('p${_next++}', title);
    projects.add(project);
    return project.id;
  }

  @override
  Future<void> moveToTrash(String id) async {
    final project = _find(id);
    projects.remove(project);
    trashed.add(project);
  }

  @override
  Future<void> restore(String id) async {
    final project = trashed.firstWhere((p) => p.id == id);
    trashed.remove(project);
    projects.add(project);
  }

  @override
  Future<void> rename(String id, String title) async {
    final project = _find(id);
    project.titles.add(project.title);
    project.redone.clear();
    project.title = title;
  }

  @override
  Future<ProjectView> view() async {
    final project = opened!;
    return ProjectView(
      id: project.id,
      title: project.title,
      durationUs: big(4000000),
      canUndo: project.titles.isNotEmpty,
      canRedo: project.redone.isNotEmpty,
      tracks: [
        TrackView(
          kind: TrackKindView.video,
          items: [
            for (final (index, clip) in project.clips.indexed)
              TrackItemView(
                sourceInUs: big(0),
                speed: 1,
                slowZoom: project.stills.contains(clip) ? false : null,
                id: clip,
                startUs: big(index * 1000000),
                endUs: big((index + 1) * 1000000),
              ),
          ],
        ),
      ],
      media: project.media,
      sound: SoundView(
        clips: [
          for (final clip in project.clips)
            ClipSoundView(
              clipId: clip,
              volume: 1,
              muted: false,
              fadeInUs: big(0),
              fadeOutUs: big(0),
            ),
        ],
        tracks: project.audioTracks,
      ),
      look: LookView(
        clips: [
          for (final clip in project.clips)
            ClipLookView(
              clipId: clip,
              speed: 1,
              brightness: 0,
              contrast: 1,
              saturation: 1,
            ),
        ],
        transition: TransitionKindView.cut,
      ),
    );
  }

  @override
  Future<ProjectView> undo() async {
    final project = opened!;
    project.redone.add(project.title);
    project.title = project.titles.removeLast();
    return view();
  }

  @override
  Future<ProjectView> redo() async {
    final project = opened!;
    project.titles.add(project.title);
    project.title = project.redone.removeLast();
    return view();
  }

  @override
  Stream<SaveStateView> saveEvents() => saves.stream;

  @override
  Stream<ThumbnailView> filmstrip(String mediaId) => const Stream.empty();

  final cropMoves = <(String, double, double, double)>[];
  final cropCommits = <String>[];
  final cropResets = <String>[];
  final slowZooms = <String>[];
  final audioCalls = <String>[];

  Future<ProjectView> _audio(String call) async {
    audioCalls.add(call);
    return view();
  }

  @override
  Future<String?> importOne(String path) async => 'media-$path';

  @override
  Future<ProjectView> addAudio(
    String mediaId,
    AudioRoleView role,
    int startUs,
  ) => _audio('add $mediaId ${role.name} $startUs');

  @override
  Future<ProjectView> setClipSound(
    String id, {
    required double volume,
    required bool muted,
    required int fadeInUs,
    required int fadeOutUs,
  }) => _audio('clip $id $volume $muted $fadeInUs $fadeOutUs');

  @override
  Future<ProjectView> setAudioSound(
    String id, {
    required double volume,
    required int fadeInUs,
    required int fadeOutUs,
  }) => _audio('sound $id $volume $fadeInUs $fadeOutUs');

  @override
  Future<ProjectView> setAudioLoop(String id, {required bool looped}) =>
      _audio('loop $id $looped');

  @override
  Future<ProjectView> deleteAudio(String id) => _audio('delete $id');

  @override
  Future<ProjectView> addAudioTrack() => _audio('add track');

  @override
  Future<ProjectView> removeAudioTrack(String id) => _audio('remove $id');

  @override
  Future<ProjectView> renameAudioTrack(String id, String name) =>
      _audio('rename $id $name');

  @override
  Future<ProjectView> setAudioTrackMuted(String? id, {required bool muted}) =>
      _audio('mute $id $muted');

  @override
  Future<Uint8List> peaks(String mediaId) async => Uint8List(0);

  final rangeCalls = <String>[];
  var cleared = 0;

  Future<ProjectView> _range(String call) async {
    rangeCalls.add(call);
    return view();
  }

  @override
  Future<ProjectView> deleteRange(int startUs, int endUs) =>
      _range('delete $startUs $endUs');

  @override
  Future<ProjectView> muteRange(int startUs, int endUs) =>
      _range('mute $startUs $endUs');

  @override
  Future<ProjectView> splitAllAt(int timeUs) => _range('split $timeUs');

  @override
  Future<int> cacheBytes() async => 3 * 1024 * 1024;

  @override
  Future<void> clearCaches() async => cleared++;

  final lookCalls = <String>[];

  Future<ProjectView> _look(String call) async {
    lookCalls.add(call);
    return view();
  }

  @override
  Future<ProjectView> setClipSpeed(String id, double speed) =>
      _look('speed $id $speed');

  @override
  Future<ProjectView> setClipColor(
    String id, {
    required double brightness,
    required double contrast,
    required double saturation,
  }) => _look('color $id $brightness $contrast $saturation');

  @override
  Future<ProjectView> setTransitionForAll(TransitionKindView kind) =>
      _look('transition ${kind.name}');

  @override
  Future<ProjectView> toggleSlowZoom(String id) async {
    slowZooms.add(id);
    return view();
  }

  @override
  void moveCrop(String id, double dx, double dy, double zoomBy) =>
      cropMoves.add((id, dx, dy, zoomBy));

  @override
  Future<ProjectView> commitCrop(String id) async {
    cropCommits.add(id);
    return view();
  }

  @override
  Future<ProjectView> resetCrop(String id) async {
    cropResets.add(id);
    return view();
  }

  /// Paths containing `bad` fail, like a corrupt file.
  @override
  Future<int> importFiles(List<String> paths) async {
    var failed = 0;
    for (final path in paths) {
      if (path.contains('bad')) {
        failed++;
      } else {
        opened!.media.add(
          MediaItemView(
            id: path,
            name: path,
            kind: MediaKindView.video,
            durationUs: big(1000000),
            offline: path.contains('gone'),
          ),
        );
      }
    }
    return failed;
  }

  @override
  Future<ProjectView> placeClips(List<String> mediaIds) async {
    opened!.clips.addAll(mediaIds);
    return view();
  }

  final clipCalls = <String>[];
  final exportEvents = StreamController<TaskEvent>();
  final exports = <String>[];
  final cancelled = <BigInt>[];

  @override
  Stream<TaskEvent> export(
    String path, {
    required int width,
    required int fps,
  }) {
    exports.add('$path $width $fps');
    return exportEvents.stream;
  }

  @override
  void cancelTask(BigInt id) => cancelled.add(id);

  @override
  Future<ProjectView> splitClip(String id, int timeUs) async {
    clipCalls.add('split $id $timeUs');
    return view();
  }

  @override
  Future<ProjectView> deleteClip(String id) async {
    clipCalls.add('delete $id');
    opened!.clips.remove(id);
    return view();
  }

  @override
  Future<ProjectView> duplicateClip(String id) async {
    clipCalls.add('duplicate $id');
    return view();
  }

  @override
  Future<ProjectView> moveClip(String id, int to) async {
    clipCalls.add('move $id $to');
    return view();
  }

  @override
  Future<ProjectView> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  }) async {
    clipCalls.add('trim $id ${start ? 'start' : 'end'} $deltaUs');
    return view();
  }

  @override
  Future<ProjectView> relink(String mediaId, String path) async {
    final media = opened!.media;
    final index = media.indexWhere((m) => m.id == mediaId);
    media[index] = MediaItemView(
      id: mediaId,
      name: path,
      kind: MediaKindView.video,
      durationUs: big(1000000),
      offline: false,
    );
    return view();
  }
}
