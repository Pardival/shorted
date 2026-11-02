import 'package:flutter/foundation.dart';

import '../../bridge/api.dart' as tasks;
import '../../bridge/api/ai.dart' as ai;
import '../../bridge/api/arrange.dart' as arrange;
import '../../bridge/api/audio.dart' as audio;
import '../../bridge/api/brand_kit.dart' as brand;
import '../../bridge/api/clips.dart' as clips;
import '../../bridge/api/export.dart' as exporter;
import '../../bridge/api/library.dart' as catalog;
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
  Future<void> exportBundle(String id, String path);

  /// Returns the new project's id.
  Future<String> importBundle(String path);
  Future<void> restore(String id);
  Future<void> rename(String id, String title);
  Future<ProjectView> view();
  Future<ProjectView> undo();

  /// Smart arrange: analysis in the background, then one edit to review.
  Stream<TaskEvent> analyseForArrange();
  Future<arrange.ArrangeReviewView> applyArrangement();
  Future<ProjectView> undoArrangement();
  Future<arrange.AiPayloadView> prepareAiArrangement(
    String language, {
    String? instruction,
  });
  Stream<TaskEvent> sendAiArrangement();
  bool aiArrangementFellBack();
  bool aiConsentRemembered(String projectId);
  Future<void> rememberAiConsent(String projectId);
  Future<List<VersionView>> versionHistory();
  Future<ProjectView> restoreVersion(String name);
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

  Future<ai.AiSettingsView> aiSettings();
  Future<ai.AiSettingsView> saveAiSettings(
    ai.AiProviderView provider, {
    required String model,
    required String baseUrl,
    String? key,
  });
  Future<ai.AiSettingsView> forgetAiKey();
  Future<void> testAiConnection();
  String defaultAiModel(ai.AiProviderView provider);
  Stream<TaskEvent> downloadSpeechModel();
  Future<void> deleteSpeechModel();
  Stream<TaskEvent> transcribe();

  /// Whether an OpenAI key is stored for transcription in the cloud (T-1201).
  bool openaiTranscriptionReady();
  Future<void> saveOpenaiKey(String key);
  Stream<TaskEvent> transcribeWithOpenai();
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

  /// The caption word at `x`, `y` (fractions of the frame) at `timeUs` (T-1202).
  Future<speech.CaptionHitView?> captionWordAt(int timeUs, double x, double y);
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
  Future<ProjectView> addText(String content, int atUs);
  Future<ProjectView> setText(String id, overlays.TextElementView text);
  Future<ProjectView> setPresetAnim(
    String id,
    overlays.AnimSlotView slot,
    overlays.PresetKindView? kind,
    int durationUs,
  );
  overlays.TransformView? overlayTransformAt(String id, int timeUs);
  void previewOverlayKey(
    String id,
    int timeUs,
    overlays.TransformView transform,
  );
  Future<ProjectView> keyOverlayTransform(
    String id,
    int timeUs,
    overlays.TransformView transform,
  );
  Future<ProjectView> removeKeyframesAt(String id, int timeUs);
  Future<ProjectView> moveKeyframes(String id, int fromUs, int toUs);
  Future<ProjectView> setKeyframeEasing(
    String id,
    int timeUs,
    overlays.EasingView easing,
  );
  Future<ProjectView> setRecordedPath(
    String id,
    List<overlays.PathPointView> samples,
  );
  Future<ProjectView> setPathOptions(
    String id, {
    required double smoothing,
    required bool orientToPath,
  });
  Future<ProjectView> movePathPoint(String id, int index, double x, double y);
  Future<ProjectView> deletePathPoint(String id, int index);
  Future<ProjectView> stretchPath(String id, int lengthUs);
  Future<ProjectView> clearPath(String id);
  List<overlays.PathPointView> pathCurve(String id);
  overlays.PresetSampleView presetSample(
    overlays.PresetKindView kind,
    overlays.AnimSlotView slot,
    double progress,
  );
  Future<ProjectView> setOverlayTiming(String id, int startUs, int endUs);
  Future<ProjectView> shiftOverlayLayer(String id, {required bool forward});
  Future<ProjectView> setOverlayFlags(
    String id, {
    required bool locked,
    required bool hidden,
  });
  Future<ProjectView> duplicateOverlay(String id);

  /// Several elements as one edit (T-1206).
  Future<ProjectView> moveOverlays(List<String> ids, double dx, double dy);
  Future<ProjectView> duplicateOverlays(List<String> ids);
  Future<ProjectView> deleteOverlays(List<String> ids);

  /// A shape's fill or a picture's tint; null gives a picture its own colours (T-1205).
  Future<ProjectView> setOverlayColor(String id, String? color);
  Future<ProjectView> deleteOverlay(String id);

  /// Where the elements on screen are drawn at `timeUs`, top first.
  List<overlays.OverlayBoxView> overlayBoxes(int timeUs);

  List<catalog.LibraryItemView> libraryItems(String query, String? category);
  catalog.PictureView? libraryThumbnail(String name, int size);
  Future<ProjectView> addLibraryItem(
    String name,
    int atUs, {
    double? x,
    double? y,
  });

  Future<List<brand.BrandAssetView>> brandKit();
  Future<List<media.ImportOutcome>> importBrandAssets(List<String> paths);
  Future<void> deleteBrandAsset(String id);
  catalog.PictureView? brandThumbnail(String id, int size);
  Future<ProjectView> addBrandAsset(
    String id,
    int atUs, {
    double? x,
    double? y,
  });

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
  Future<void> exportBundle(String id, String path) =>
      engine.exportProjectBundle(root: root, id: id, path: path);

  @override
  Future<String> importBundle(String path) =>
      engine.importProjectBundle(root: root, path: path);

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
  Stream<TaskEvent> analyseForArrange() => arrange.analyseForArrange();

  @override
  Future<arrange.ArrangeReviewView> applyArrangement() =>
      arrange.applyArrangement();

  @override
  Future<ProjectView> undoArrangement() => arrange.undoArrangement();

  @override
  Future<arrange.AiPayloadView> prepareAiArrangement(
    String language, {
    String? instruction,
  }) => arrange.prepareAiArrangement(
    root: root,
    language: language,
    instruction: instruction,
  );

  @override
  Stream<TaskEvent> sendAiArrangement() => arrange.sendAiArrangement();

  @override
  bool aiArrangementFellBack() => arrange.aiArrangementFellBack();

  @override
  bool aiConsentRemembered(String projectId) =>
      arrange.aiConsentRemembered(root: root, projectId: projectId);

  @override
  Future<void> rememberAiConsent(String projectId) =>
      arrange.rememberAiConsent(root: root, projectId: projectId);

  @override
  Future<List<VersionView>> versionHistory() => engine.versionHistory();

  @override
  Future<ProjectView> restoreVersion(String name) =>
      engine.restoreVersion(name: name);

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
  Future<ProjectView> addText(String content, int atUs) =>
      overlays.addText(content: content, atUs: atUs);

  @override
  Future<ProjectView> setText(String id, overlays.TextElementView text) =>
      overlays.setText(id: id, text: text);

  @override
  Future<ProjectView> setPresetAnim(
    String id,
    overlays.AnimSlotView slot,
    overlays.PresetKindView? kind,
    int durationUs,
  ) => overlays.setPresetAnim(
    id: id,
    slot: slot,
    kind: kind,
    durationUs: durationUs,
  );

  @override
  overlays.TransformView? overlayTransformAt(String id, int timeUs) =>
      overlays.overlayTransformAt(id: id, timeUs: timeUs);

  @override
  void previewOverlayKey(
    String id,
    int timeUs,
    overlays.TransformView transform,
  ) => overlays.previewOverlayKey(id: id, timeUs: timeUs, transform: transform);

  @override
  Future<ProjectView> keyOverlayTransform(
    String id,
    int timeUs,
    overlays.TransformView transform,
  ) => overlays.keyOverlayTransform(
    id: id,
    timeUs: timeUs,
    transform: transform,
  );

  @override
  Future<ProjectView> removeKeyframesAt(String id, int timeUs) =>
      overlays.removeKeyframesAt(id: id, timeUs: timeUs);

  @override
  Future<ProjectView> moveKeyframes(String id, int fromUs, int toUs) =>
      overlays.moveKeyframes(id: id, fromUs: fromUs, toUs: toUs);

  @override
  Future<ProjectView> setKeyframeEasing(
    String id,
    int timeUs,
    overlays.EasingView easing,
  ) => overlays.setKeyframeEasing(id: id, timeUs: timeUs, easing: easing);

  @override
  Future<ProjectView> setRecordedPath(
    String id,
    List<overlays.PathPointView> samples,
  ) => overlays.setRecordedPath(id: id, samples: samples);

  @override
  Future<ProjectView> setPathOptions(
    String id, {
    required double smoothing,
    required bool orientToPath,
  }) => overlays.setPathOptions(
    id: id,
    smoothing: smoothing,
    orientToPath: orientToPath,
  );

  @override
  Future<ProjectView> movePathPoint(String id, int index, double x, double y) =>
      overlays.movePathPoint(id: id, index: index, x: x, y: y);

  @override
  Future<ProjectView> deletePathPoint(String id, int index) =>
      overlays.deletePathPoint(id: id, index: index);

  @override
  Future<ProjectView> stretchPath(String id, int lengthUs) =>
      overlays.stretchPath(id: id, lengthUs: lengthUs);

  @override
  Future<ProjectView> clearPath(String id) => overlays.clearPath(id: id);

  @override
  List<overlays.PathPointView> pathCurve(String id) =>
      overlays.pathCurve(id: id);

  @override
  overlays.PresetSampleView presetSample(
    overlays.PresetKindView kind,
    overlays.AnimSlotView slot,
    double progress,
  ) => overlays.presetSample(kind: kind, slot: slot, progress: progress);

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
  Future<ProjectView> setOverlayColor(String id, String? color) =>
      overlays.setOverlayColor(id: id, color: color);

  @override
  Future<ProjectView> moveOverlays(List<String> ids, double dx, double dy) =>
      overlays.moveOverlays(ids: ids, dx: dx, dy: dy);

  @override
  Future<ProjectView> duplicateOverlays(List<String> ids) =>
      overlays.duplicateOverlays(ids: ids);

  @override
  Future<ProjectView> deleteOverlays(List<String> ids) =>
      overlays.deleteOverlays(ids: ids);

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
  List<catalog.LibraryItemView> libraryItems(String query, String? category) =>
      catalog.libraryItems(query: query, category: category);

  @override
  catalog.PictureView? libraryThumbnail(String name, int size) =>
      catalog.libraryThumbnail(name: name, size: size);

  @override
  Future<ProjectView> addLibraryItem(
    String name,
    int atUs, {
    double? x,
    double? y,
  }) => catalog.addLibraryItem(name: name, atUs: atUs, x: x, y: y);

  @override
  Future<List<brand.BrandAssetView>> brandKit() => brand.brandKit(root: root);

  @override
  Future<List<media.ImportOutcome>> importBrandAssets(List<String> paths) =>
      brand.importBrandAssets(root: root, paths: paths);

  @override
  Future<void> deleteBrandAsset(String id) =>
      brand.deleteBrandAsset(root: root, id: id);

  @override
  catalog.PictureView? brandThumbnail(String id, int size) =>
      brand.brandThumbnail(root: root, id: id, size: size);

  @override
  Future<ProjectView> addBrandAsset(
    String id,
    int atUs, {
    double? x,
    double? y,
  }) => brand.addBrandAsset(root: root, id: id, atUs: atUs, x: x, y: y);

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
  Future<ai.AiSettingsView> aiSettings() => ai.aiSettings(root: root);

  @override
  Future<ai.AiSettingsView> saveAiSettings(
    ai.AiProviderView provider, {
    required String model,
    required String baseUrl,
    String? key,
  }) => ai.saveAiSettings(
    root: root,
    provider: provider,
    model: model,
    baseUrl: baseUrl,
    key: key,
  );

  @override
  Future<ai.AiSettingsView> forgetAiKey() => ai.forgetAiKey(root: root);

  @override
  Future<void> testAiConnection() => ai.testAiConnection(root: root);

  @override
  String defaultAiModel(ai.AiProviderView provider) =>
      ai.defaultAiModel(provider: provider);

  @override
  Stream<TaskEvent> downloadSpeechModel() =>
      speech.downloadSpeechModel(root: root);

  @override
  Future<void> deleteSpeechModel() => speech.deleteSpeechModel(root: root);

  @override
  Stream<TaskEvent> transcribe() => speech.transcribeProject(root: root);

  @override
  bool openaiTranscriptionReady() => speech.openaiTranscriptionReady();

  @override
  Future<void> saveOpenaiKey(String key) => speech.saveOpenaiKey(key: key);

  @override
  Stream<TaskEvent> transcribeWithOpenai() => speech.transcribeWithOpenai();

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
  Future<speech.CaptionHitView?> captionWordAt(
    int timeUs,
    double x,
    double y,
  ) => speech.captionWordAt(timeUs: timeUs, x: x, y: y);

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
