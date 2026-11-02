import 'package:flutter/foundation.dart';

import '../../bridge/api.dart' as tasks;
import '../../bridge/api/audio.dart' as audio;
import '../../bridge/api/clips.dart' as clips;
import '../../bridge/api/export.dart' as exporter;
import '../../bridge/api/look.dart' as look;
import '../../bridge/api/media.dart' as media;
import '../../bridge/api/overlays.dart' as overlays;
import '../../bridge/api/projects.dart' as engine;
import '../../bridge/api/transcribe.dart' as speech;
import '../../bridge/api/projects.dart';
import '../../bridge/runtime.dart';

/// Everything the app asks of the engine about projects. Screens use this, so widget tests can
/// run on a fake (docs/09 §1).
abstract interface class ProjectGateway {
  Future<List<ProjectSummary>> list();
  Future<List<ProjectSummary>> trash();
  Future<String> create(String title);
  Future<void> open(String id);
  Future<void> close();
  Future<String> duplicate(String id, String title);
  Future<void> moveToTrash(String id);
  Future<void> restore(String id);
  Future<void> rename(String id, String title);
  Future<ProjectView> view();
  Future<ProjectView> undo();
  Future<ProjectView> redo();
  Stream<SaveStateView> saveEvents();

  /// Returns how many files failed; the others are imported.
  Future<int> importFiles(List<String> paths);
  Future<ProjectView> placeClips(List<String> mediaIds);
  Future<ProjectView> relink(String mediaId, String path);
  Future<ProjectView> splitClip(String id, int timeUs);
  Future<ProjectView> deleteClip(String id);
  Future<ProjectView> duplicateClip(String id);
  Future<ProjectView> moveClip(String id, int to);
  Future<ProjectView> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  });
  Stream<TaskEvent> export(String path, {required int width, required int fps});
  Stream<media.ThumbnailView> filmstrip(String mediaId);
  void moveCrop(String id, double dx, double dy, double zoomBy);
  Future<ProjectView> commitCrop(String id);
  Future<ProjectView> resetCrop(String id);
  Future<ProjectView> toggleSlowZoom(String id);
  Future<String?> importOne(String path);
  Future<ProjectView> addAudio(
    String mediaId,
    audio.AudioRoleView role,
    int startUs,
  );
  Future<ProjectView> setClipSound(
    String id, {
    required double volume,
    required bool muted,
    required int fadeInUs,
    required int fadeOutUs,
  });
  Future<ProjectView> setAudioSound(
    String id, {
    required double volume,
    required int fadeInUs,
    required int fadeOutUs,
  });
  Future<ProjectView> setAudioLoop(String id, {required bool looped});
  Future<ProjectView> deleteAudio(String id);
  Future<ProjectView> setDucking(String id, double? strength);
  Future<ProjectView> moveAudio(String id, int startUs, String? trackId);
  Future<ProjectView> trimAudio(
    String id, {
    required bool start,
    required int deltaUs,
  });
  Future<ProjectView> reorderAudioTrack(String id, int to);
  Future<ProjectView> addAudioTrack();
  Future<void> startVoiceOver();
  Future<ProjectView> stopVoiceOver(int startUs);
  double get voiceLevel;
  Future<ProjectView> removeAudioTrack(String id);
  Future<ProjectView> renameAudioTrack(String id, String name);
  Future<ProjectView> setTrackFlags(
    TrackView track, {
    bool? muted,
    bool? locked,
    bool? hidden,
  });
  Future<Uint8List> peaks(String mediaId);
  Future<ProjectView> deleteRange(int startUs, int endUs);
  Future<ProjectView> muteRange(int startUs, int endUs);
  Future<ProjectView> splitAllAt(int timeUs);
  Future<ProjectView> collectMedia();
  speech.ModelView speechModel();
  Stream<TaskEvent> downloadSpeechModel();
  Future<void> deleteSpeechModel();
  Stream<TaskEvent> transcribe();
  Future<ProjectView> editWord(
    speech.TranscriptView transcript,
    String segment,
    int word,
    String text,
  );
  Future<ProjectView> hideSegment(
    speech.TranscriptView transcript,
    String segment, {
    required bool hidden,
  });
  Future<ProjectView> toggleEmphasis(
    speech.TranscriptView transcript,
    String segment,
    int word,
  );
  Future<ProjectView> setCaptionPreset(speech.CaptionPresetView preset);
  Future<ProjectView> setCaptionsEnabled({required bool enabled});
  Future<ProjectView> setCaptionAnimation(
    speech.CaptionAnimationView animation,
  );
  Future<ProjectView> setCaptionStyle({
    required String font,
    required double size,
    required bool uppercase,
    required String color,
    required String emphasisColor,
    required double anchor,
  });
  Future<void> writeSrt(String path);
  Future<media.ThumbnailView?> cover(String projectId);
  int exportSizeEstimate(int width, int fps, int durationUs);
  Future<int> cacheBytes();
  Future<void> clearCaches();
  Future<ProjectView> setClipSpeed(String id, double speed);
  Future<ProjectView> setClipColor(
    String id, {
    required double brightness,
    required double contrast,
    required double saturation,
  });
  Future<ProjectView> setTransitionForAll(look.TransitionKindView kind);
  void cancelTask(BigInt id);

  Future<ProjectView> addShape(overlays.ShapeKindView shape, int atUs);
  Future<ProjectView> setOverlayTiming(String id, int startUs, int endUs);
  Future<ProjectView> shiftOverlayLayer(String id, {required bool forward});
  Future<ProjectView> setOverlayFlags(
    String id, {
    required bool locked,
    required bool hidden,
  });
  Future<ProjectView> duplicateOverlay(String id);
  Future<ProjectView> deleteOverlay(String id);

  /// Where the elements on screen are drawn at `timeUs`, top first.
  List<overlays.OverlayBoxView> overlayBoxes(int timeUs);

  /// Shown during a gesture, not recorded.
  void previewOverlayTransform(String id, overlays.TransformView transform);
  Future<ProjectView> setOverlayTransform(
    String id,
    overlays.TransformView transform,
  );
}

class EngineGateway implements ProjectGateway {
  EngineGateway(this.root);

  final String root;

  @override
  Future<List<ProjectSummary>> list() => engine.listProjects(root: root);

  @override
  Future<List<ProjectSummary>> trash() => engine.listTrash(root: root);

  @override
  Future<String> create(String title) =>
      engine.createProject(root: root, title: title);

  @override
  Future<void> open(String id) => engine.openProject(root: root, id: id);

  @override
  Future<void> close() => engine.closeProject();

  @override
  Future<String> duplicate(String id, String title) =>
      engine.duplicateProject(root: root, id: id, title: title);

  @override
  Future<void> moveToTrash(String id) =>
      engine.trashProject(root: root, id: id);

  @override
  Future<void> restore(String id) => engine.restoreProject(root: root, id: id);

  @override
  Future<void> rename(String id, String title) =>
      engine.renameProject(root: root, id: id, title: title);

  @override
  Future<ProjectView> view() => engine.projectView();

  @override
  Future<ProjectView> undo() => engine.undo();

  @override
  Future<ProjectView> redo() => engine.redo();

  @override
  Stream<SaveStateView> saveEvents() => engine.saveEvents();

  @override
  Future<int> importFiles(List<String> paths) async {
    final outcomes = await media.importFiles(paths: paths);
    return outcomes.where((outcome) => outcome.error != null).length;
  }

  @override
  Future<ProjectView> placeClips(List<String> mediaIds) =>
      media.placeClips(mediaIds: mediaIds);

  @override
  Future<ProjectView> relink(String mediaId, String path) =>
      media.relinkMedia(id: mediaId, path: path);

  @override
  Future<ProjectView> splitClip(String id, int timeUs) =>
      clips.splitClip(id: id, timeUs: timeUs);

  @override
  Future<ProjectView> deleteClip(String id) => clips.deleteClip(id: id);

  @override
  Future<ProjectView> duplicateClip(String id) => clips.duplicateClip(id: id);

  @override
  Future<ProjectView> moveClip(String id, int to) =>
      clips.moveClip(id: id, to: to);

  @override
  Future<ProjectView> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  }) => clips.trimClip(
    id: id,
    edge: start ? clips.ClipEdge.start : clips.ClipEdge.end,
    deltaUs: deltaUs,
  );

  @override
  Stream<TaskEvent> export(
    String path, {
    required int width,
    required int fps,
  }) => exporter.startExport(
    path: path,
    settings: exporter.ExportSettings(width: width, fps: fps),
  );

  @override
  void moveCrop(String id, double dx, double dy, double zoomBy) =>
      clips.moveCrop(id: id, dx: dx, dy: dy, zoomBy: zoomBy);

  @override
  Future<ProjectView> commitCrop(String id) => clips.commitCrop(id: id);

  @override
  Future<ProjectView> resetCrop(String id) => clips.resetCrop(id: id);

  @override
  Future<ProjectView> toggleSlowZoom(String id) => clips.toggleSlowZoom(id: id);

  @override
  Future<String?> importOne(String path) async {
    final outcomes = await media.importFiles(paths: [path]);
    return outcomes.single.mediaId;
  }

  @override
  Future<ProjectView> addAudio(
    String mediaId,
    audio.AudioRoleView role,
    int startUs,
  ) => audio.addAudio(mediaId: mediaId, role: role, startUs: startUs);

  @override
  Future<ProjectView> setClipSound(
    String id, {
    required double volume,
    required bool muted,
    required int fadeInUs,
    required int fadeOutUs,
  }) => audio.setClipSound(
    id: id,
    volume: volume,
    muted: muted,
    fadeInUs: fadeInUs,
    fadeOutUs: fadeOutUs,
  );

  @override
  Future<ProjectView> setAudioSound(
    String id, {
    required double volume,
    required int fadeInUs,
    required int fadeOutUs,
  }) => audio.setAudioSound(
    id: id,
    volume: volume,
    fadeInUs: fadeInUs,
    fadeOutUs: fadeOutUs,
  );

  @override
  Future<ProjectView> setAudioLoop(String id, {required bool looped}) =>
      audio.setAudioLoop(id: id, looped: looped);

  @override
  Future<ProjectView> deleteAudio(String id) => audio.deleteAudio(id: id);

  @override
  Future<ProjectView> setDucking(String id, double? strength) =>
      audio.setDucking(id: id, strength: strength);

  @override
  Future<ProjectView> moveAudio(String id, int startUs, String? trackId) =>
      audio.moveAudio(id: id, startUs: startUs, trackId: trackId);

  @override
  Future<ProjectView> trimAudio(
    String id, {
    required bool start,
    required int deltaUs,
  }) => audio.trimAudio(id: id, start: start, deltaUs: deltaUs);

  @override
  Future<ProjectView> reorderAudioTrack(String id, int to) =>
      audio.reorderAudioTrack(id: id, to: to);

  @override
  Future<ProjectView> addAudioTrack() => audio.addAudioTrack();

  @override
  Future<void> startVoiceOver() => audio.startVoiceOver();

  @override
  Future<ProjectView> stopVoiceOver(int startUs) =>
      audio.stopVoiceOver(startUs: startUs);

  @override
  double get voiceLevel => audio.voiceLevel();

  @override
  Future<ProjectView> removeAudioTrack(String id) =>
      audio.removeAudioTrack(id: id);

  @override
  Future<ProjectView> renameAudioTrack(String id, String name) =>
      audio.renameAudioTrack(id: id, name: name);

  @override
  Future<ProjectView> setTrackFlags(
    TrackView track, {
    bool? muted,
    bool? locked,
    bool? hidden,
  }) => audio.setTrackFlags(
    kind: track.kind,
    id: track.id,
    muted: muted ?? track.muted,
    locked: locked ?? track.locked,
    hidden: hidden ?? track.hidden,
  );

  @override
  Future<ProjectView> setClipSpeed(String id, double speed) =>
      look.setClipSpeed(id: id, speed: speed);

  @override
  Future<ProjectView> setClipColor(
    String id, {
    required double brightness,
    required double contrast,
    required double saturation,
  }) => look.setClipColor(
    id: id,
    brightness: brightness,
    contrast: contrast,
    saturation: saturation,
  );

  @override
  Future<ProjectView> setTransitionForAll(look.TransitionKindView kind) =>
      look.setTransitionForAll(kind: kind);

  @override
  Future<ProjectView> addShape(overlays.ShapeKindView shape, int atUs) =>
      overlays.addShape(shape: shape, atUs: atUs);

  @override
  Future<ProjectView> setOverlayTiming(String id, int startUs, int endUs) =>
      overlays.setOverlayTiming(id: id, startUs: startUs, endUs: endUs);

  @override
  Future<ProjectView> shiftOverlayLayer(String id, {required bool forward}) =>
      overlays.shiftOverlayLayer(id: id, forward: forward);

  @override
  Future<ProjectView> setOverlayFlags(
    String id, {
    required bool locked,
    required bool hidden,
  }) => overlays.setOverlayFlags(id: id, locked: locked, hidden: hidden);

  @override
  Future<ProjectView> duplicateOverlay(String id) =>
      overlays.duplicateOverlay(id: id);

  @override
  Future<ProjectView> deleteOverlay(String id) =>
      overlays.deleteOverlay(id: id);

  @override
  List<overlays.OverlayBoxView> overlayBoxes(int timeUs) =>
      overlays.overlayBoxes(timeUs: timeUs);

  @override
  void previewOverlayTransform(String id, overlays.TransformView transform) =>
      overlays.previewOverlayTransform(id: id, transform: transform);

  @override
  Future<ProjectView> setOverlayTransform(
    String id,
    overlays.TransformView transform,
  ) => overlays.setOverlayTransform(id: id, transform: transform);

  @override
  Future<ProjectView> deleteRange(int startUs, int endUs) =>
      clips.deleteRange(startUs: startUs, endUs: endUs);

  @override
  Future<ProjectView> muteRange(int startUs, int endUs) =>
      clips.muteRange(startUs: startUs, endUs: endUs);

  @override
  Future<ProjectView> splitAllAt(int timeUs) =>
      clips.splitAllAt(timeUs: timeUs);

  @override
  Future<ProjectView> collectMedia() => media.collectMedia();

  @override
  speech.ModelView speechModel() => speech.speechModel(root: root);

  @override
  Stream<TaskEvent> downloadSpeechModel() =>
      speech.downloadSpeechModel(root: root);

  @override
  Future<void> deleteSpeechModel() => speech.deleteSpeechModel(root: root);

  @override
  Stream<TaskEvent> transcribe() => speech.transcribeProject(root: root);

  @override
  Future<ProjectView> editWord(
    speech.TranscriptView transcript,
    String segment,
    int word,
    String text,
  ) => speech.editWord(
    clipId: transcript.clipId,
    audioId: transcript.audioId,
    segment: segment,
    word: word,
    text: text,
  );

  @override
  Future<ProjectView> toggleEmphasis(
    speech.TranscriptView transcript,
    String segment,
    int word,
  ) => speech.toggleEmphasis(
    clipId: transcript.clipId,
    audioId: transcript.audioId,
    segment: segment,
    word: word,
  );

  @override
  Future<ProjectView> setCaptionPreset(speech.CaptionPresetView preset) =>
      speech.setCaptionPreset(preset: preset);

  @override
  Future<ProjectView> setCaptionsEnabled({required bool enabled}) =>
      speech.setCaptionsEnabled(enabled: enabled);

  @override
  Future<ProjectView> setCaptionAnimation(
    speech.CaptionAnimationView animation,
  ) => speech.setCaptionAnimation(animation: animation);

  @override
  Future<ProjectView> setCaptionStyle({
    required String font,
    required double size,
    required bool uppercase,
    required String color,
    required String emphasisColor,
    required double anchor,
  }) => speech.setCaptionStyle(
    font: font,
    size: size,
    uppercase: uppercase,
    color: color,
    emphasisColor: emphasisColor,
    anchor: anchor,
  );

  @override
  Future<void> writeSrt(String path) => speech.writeSrt(path: path);

  @override
  Future<ProjectView> hideSegment(
    speech.TranscriptView transcript,
    String segment, {
    required bool hidden,
  }) => speech.hideSegment(
    clipId: transcript.clipId,
    audioId: transcript.audioId,
    segment: segment,
    hidden: hidden,
  );

  @override
  Future<media.ThumbnailView?> cover(String projectId) =>
      media.projectCover(root: root, id: projectId);

  @override
  int exportSizeEstimate(int width, int fps, int durationUs) =>
      exporter.exportSizeEstimate(
        settings: exporter.ExportSettings(width: width, fps: fps),
        durationUs: durationUs,
      );

  @override
  Future<int> cacheBytes() async =>
      (await media.cacheBytes(root: root)).toInt();

  @override
  Future<void> clearCaches() => media.clearCaches(root: root);

  @override
  Future<Uint8List> peaks(String mediaId) => audio.mediaPeaks(mediaId: mediaId);

  @override
  Stream<media.ThumbnailView> filmstrip(String mediaId) =>
      media.mediaFilmstrip(mediaId: mediaId);

  @override
  void cancelTask(BigInt id) => tasks.cancelTask(taskId: id);
}
