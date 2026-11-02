import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/arrange.dart';
import '../../bridge/api/projects.dart';
import '../preview/preview_controller.dart';
import 'project_gateway.dart';

/// Overridden at startup, once the data folder is known.
final projectGatewayProvider = Provider<ProjectGateway>(
  (ref) => throw UnimplementedError('projectGatewayProvider is not set'),
);

/// Overridden at startup, like the gateway.
/// The clip being reframed on the preview, if any.
class CropTarget extends Notifier<({String projectId, String clipId})?> {
  @override
  ({String projectId, String clipId})? build() => null;

  void set(({String projectId, String clipId})? target) => state = target;
}

final cropTargetProvider =
    NotifierProvider.autoDispose<
      CropTarget,
      ({String projectId, String clipId})?
    >(CropTarget.new);

/// True while a voice-over records: the video plays for the whole take.
class RecordingVoice extends Notifier<bool> {
  @override
  bool build() => false;

  void set(bool recording) => state = recording;
}

final recordingVoiceProvider = NotifierProvider<RecordingVoice, bool>(
  RecordingVoice.new,
);

/// The clip selected on the timeline, for the panels that edit one clip.
class SelectedClip extends Notifier<String?> {
  @override
  String? build() => null;

  void set(String? id) => state = id;
}

final selectedClipProvider =
    NotifierProvider.autoDispose<SelectedClip, String?>(SelectedClip.new);

/// Expert mode of the Animate sheet: a change at the playhead becomes a keyframe (T-0707).
class KeyframeMode extends Notifier<bool> {
  @override
  bool build() => false;

  void set(bool on) => state = on;
}

final keyframeModeProvider = NotifierProvider<KeyframeMode, bool>(
  KeyframeMode.new,
);

/// A keyframe picked on the Widgets track: the element and the time of its keys.
class SelectedKeyframe extends Notifier<({String overlayId, int timeUs})?> {
  @override
  ({String overlayId, int timeUs})? build() => null;

  void set(({String overlayId, int timeUs})? keyframe) => state = keyframe;
}

final selectedKeyframeProvider =
    NotifierProvider.autoDispose<
      SelectedKeyframe,
      ({String overlayId, int timeUs})?
    >(SelectedKeyframe.new);

/// The element whose recorded path is shown and edited on the preview (T-0708).
class PathEditing extends Notifier<String?> {
  @override
  String? build() => null;

  void set(String? id) => state = id;
}

final pathEditingProvider = NotifierProvider.autoDispose<PathEditing, String?>(
  PathEditing.new,
);

/// Set while a path recording waits for the finger: the playback speed.
class RecordingPath extends Notifier<double?> {
  @override
  double? build() => null;

  void set(double? rate) => state = rate;
}

final recordingPathProvider =
    NotifierProvider.autoDispose<RecordingPath, double?>(RecordingPath.new);

/// The overlay selected on the preview or the Widgets track.
class SelectedOverlay extends Notifier<String?> {
  @override
  String? build() => null;

  void set(String? id) => state = id;
}

final selectedOverlayProvider =
    NotifierProvider.autoDispose<SelectedOverlay, String?>(SelectedOverlay.new);

/// The sound (music, voice) selected on the timeline (T-1204).
class SelectedSound extends Notifier<String?> {
  @override
  String? build() => null;

  void set(String? id) => state = id;
}

final selectedSoundProvider =
    NotifierProvider.autoDispose<SelectedSound, String?>(SelectedSound.new);

final previewTransportProvider = Provider<PreviewTransport>(
  (ref) => throw UnimplementedError('previewTransportProvider is not set'),
);

final projectListProvider =
    AsyncNotifierProvider<ProjectList, List<ProjectSummary>>(ProjectList.new);

class ProjectList extends AsyncNotifier<List<ProjectSummary>> {
  ProjectGateway get _gateway => ref.read(projectGatewayProvider);

  @override
  Future<List<ProjectSummary>> build() => _gateway.list();

  Future<void> _refresh() async {
    state = AsyncData(await _gateway.list());
  }

  Future<String> create(String title) async {
    final id = await _gateway.create(title);
    await _refresh();
    return id;
  }

  Future<void> rename(String id, String title) async {
    await _gateway.rename(id, title);
    await _refresh();
  }

  Future<void> duplicate(String id, String title) async {
    await _gateway.duplicate(id, title);
    await _refresh();
  }

  Future<void> exportBundle(String id, String path) =>
      _gateway.exportBundle(id, path);

  Future<String> importBundle(String path) async {
    final id = await _gateway.importBundle(path);
    await _refresh();
    return id;
  }

  Future<void> moveToTrash(String id) async {
    await _gateway.moveToTrash(id);
    await _refresh();
  }

  Future<void> restore(String id) async {
    await _gateway.restore(id);
    await _refresh();
  }

  Future<void> reload() => _refresh();
}

final trashProvider = FutureProvider.autoDispose<List<ProjectSummary>>(
  (ref) => ref.read(projectGatewayProvider).trash(),
);

class EditorState {
  const EditorState({required this.view, required this.save});

  final ProjectView view;
  final SaveStateView save;

  EditorState copyWith({ProjectView? view, SaveStateView? save}) =>
      EditorState(view: view ?? this.view, save: save ?? this.save);
}

final editorProvider = AsyncNotifierProvider.autoDispose
    .family<EditorController, EditorState, String>(EditorController.new);

/// The open project. The engine owns the data: every action returns a new view from it.
class EditorController extends AsyncNotifier<EditorState> {
  EditorController(this.projectId);

  final String projectId;

  ProjectGateway get _gateway => ref.read(projectGatewayProvider);

  @override
  Future<EditorState> build() async {
    final saves = _gateway.saveEvents().listen((save) {
      final current = state.value;
      if (current != null) state = AsyncData(current.copyWith(save: save));
    });
    ref.onDispose(saves.cancel);
    await _gateway.open(projectId);
    return EditorState(view: await _gateway.view(), save: SaveStateView.saved);
  }

  void _show(ProjectView view) {
    final current = state.value;
    if (current != null) state = AsyncData(current.copyWith(view: view));
  }

  /// Applies the plan smart arrange made, as one edit, and returns what it did for review.
  Future<ArrangeReviewView> arrange() async {
    final review = await _gateway.applyArrangement();
    _show(review.project);
    return review;
  }

  /// A background task changed the project: shows the result.
  Future<void> reload() async => _show(await _gateway.view());

  /// Any edit that answers with a new view.
  Future<void> apply(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  ) async => _show(await action(_gateway));

  Future<void> undo() async => _show(await _gateway.undo());

  Future<void> redo() async => _show(await _gateway.redo());

  Future<void> rename(String title) async {
    await _gateway.rename(projectId, title);
    _show(await _gateway.view());
  }

  /// Returns how many files failed.
  Future<int> importFiles(List<String> paths) async {
    final failed = await _gateway.importFiles(paths);
    _show(await _gateway.view());
    return failed;
  }

  Future<void> placeClips(List<String> mediaIds) async =>
      _show(await _gateway.placeClips(mediaIds));

  Future<void> relink(String mediaId, String path) async =>
      _show(await _gateway.relink(mediaId, path));

  Future<void> splitClip(String id, int timeUs) async =>
      _show(await _gateway.splitClip(id, timeUs));

  Future<void> deleteClip(String id) async =>
      _show(await _gateway.deleteClip(id));

  Future<void> duplicateClip(String id) async =>
      _show(await _gateway.duplicateClip(id));

  Future<void> commitCrop(String id) async =>
      _show(await _gateway.commitCrop(id));

  Future<void> resetCrop(String id) async =>
      _show(await _gateway.resetCrop(id));

  Future<void> toggleSlowZoom(String id) async =>
      _show(await _gateway.toggleSlowZoom(id));

  Future<void> moveClip(String id, int to) async =>
      _show(await _gateway.moveClip(id, to));

  Future<void> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  }) async =>
      _show(await _gateway.trimClip(id, start: start, deltaUs: deltaUs));
}
