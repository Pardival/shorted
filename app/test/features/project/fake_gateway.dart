import 'dart:async';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:shorted/bridge/api/ai.dart';
import 'package:shorted/bridge/api/arrange.dart';
import 'package:shorted/bridge/api/audio.dart';
import 'package:shorted/bridge/api/brand_kit.dart';
import 'package:shorted/bridge/api/library.dart';
import 'package:shorted/bridge/api/look.dart';
import 'package:shorted/bridge/api/media.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/api/transcribe.dart';
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
  final transcripts = <TranscriptView>[];

  /// Bottom first.
  final overlays = <OverlayView>[];

  /// Items per user audio track, as timeline items.
  final sounds = <String, List<String>>{};
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
        // The engine always has it; here only when used, so other tests keep their layout.
        if (project.overlays.isNotEmpty)
          TrackView(
            kind: TrackKindView.widgets,
            muted: false,
            locked: false,
            hidden: false,
            items: [
              for (final overlay in project.overlays)
                TrackItemView(
                  sourceInUs: big(0),
                  speed: 1,
                  id: overlay.id,
                  startUs: overlay.startUs,
                  endUs: overlay.endUs,
                ),
            ],
          ),
        TrackView(
          kind: TrackKindView.video,
          muted: false,
          locked: false,
          hidden: false,
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
        for (final track in project.audioTracks)
          TrackView(
            kind: TrackKindView.audio,
            id: track.id,
            name: track.name,
            muted: track.muted,
            locked: false,
            hidden: false,
            items: [
              for (final (index, sound)
                  in (project.sounds[track.id] ?? []).indexed)
                TrackItemView(
                  sourceInUs: big(0),
                  speed: 1,
                  id: sound,
                  startUs: big(index * 2000000),
                  endUs: big(index * 2000000 + 1000000),
                ),
            ],
          ),
      ],
      media: project.media,
      overlays: List.of(project.overlays),
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
      transcripts: project.transcripts,
      captions: const CaptionsView(
        enabled: true,
        preset: CaptionPresetView.wordByWord,
        animation: CaptionAnimationView.pop,
        font: 'Space Grotesk',
        size: 96,
        uppercase: true,
        color: '#FFFFFFFF',
        emphasisColor: '#C6F135FF',
        anchor: 0.7,
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

  /// What the next smart arrange finds; the analysis reports these events.
  List<DecisionView> decisions = [];
  List<UncertainView> uncertain = [];
  final arrangeCalls = <String>[];
  StreamController<TaskEvent>? arranging;

  @override
  Stream<TaskEvent> analyseForArrange() {
    arrangeCalls.add('analyse');
    final events = StreamController<TaskEvent>();
    arranging = events;
    return events.stream;
  }

  @override
  Future<ArrangeReviewView> applyArrangement() async {
    arrangeCalls.add('apply');
    return ArrangeReviewView(
      beforeUs: big(166000000),
      afterUs: big(64000000),
      summary: '',
      decisions: decisions,
      uncertain: uncertain,
      project: await view(),
    );
  }

  final consented = <String>{};
  bool fellBack = false;
  static const payload =
      '{"model":"m","messages":[{"role":"user","content":"Clips: c1"}]}';

  @override
  Future<AiPayloadView> prepareAiArrangement(String language) async {
    arrangeCalls.add('prepare $language');
    final code = aiTestFails;
    if (code != null) throw EngineFailure(code: code);
    return const AiPayloadView(
      provider: 'anthropic',
      model: 'claude-sonnet-5-5',
      url: 'https://api.anthropic.com/v1/messages',
      body: payload,
    );
  }

  @override
  Stream<TaskEvent> sendAiArrangement() {
    arrangeCalls.add('send');
    final events = StreamController<TaskEvent>();
    arranging = events;
    return events.stream;
  }

  @override
  bool aiArrangementFellBack() => fellBack;

  @override
  bool aiConsentRemembered(String projectId) => consented.contains(projectId);

  @override
  Future<void> rememberAiConsent(String projectId) async =>
      consented.add(projectId);

  @override
  Future<ProjectView> undoArrangement() {
    arrangeCalls.add('undo');
    return view();
  }

  AiSettingsView ai = const AiSettingsView(
    model: '',
    baseUrl: '',
    hasKey: false,
  );
  final aiCalls = <String>[];
  ErrorCode? aiTestFails;

  @override
  Future<AiSettingsView> aiSettings() async => ai;

  @override
  Future<AiSettingsView> saveAiSettings(
    AiProviderView provider, {
    required String model,
    required String baseUrl,
    String? key,
  }) async {
    // Only whether a key came is recorded: a test must not keep it either.
    aiCalls.add('save ${provider.name} $model $baseUrl ${key != null}');
    ai = AiSettingsView(
      provider: provider,
      model: model,
      baseUrl: baseUrl,
      hasKey: ai.hasKey || key != null,
    );
    return ai;
  }

  @override
  Future<AiSettingsView> forgetAiKey() async {
    aiCalls.add('forget');
    ai = AiSettingsView(
      provider: ai.provider,
      model: ai.model,
      baseUrl: ai.baseUrl,
      hasKey: false,
    );
    return ai;
  }

  @override
  Future<void> testAiConnection() async {
    aiCalls.add('test');
    final code = aiTestFails;
    if (code != null) throw EngineFailure(code: code);
  }

  @override
  String defaultAiModel(AiProviderView provider) => '${provider.name}-default';

  final versions = <VersionView>[];
  final restored = <String>[];

  @override
  Future<List<VersionView>> versionHistory() async => List.of(versions);

  @override
  Future<ProjectView> restoreVersion(String name) {
    restored.add(name);
    return view();
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

  final overlayCalls = <String>[];
  List<OverlayBoxView> boxes = [];

  Future<ProjectView> _overlay(String call) async {
    overlayCalls.add(call);
    return view();
  }

  @override
  Future<ProjectView> addShape(ShapeKindView shape, int atUs) {
    opened!.overlays.add(
      OverlayView(
        path: null,
        keyframes: const [],
        animIn: null,
        animLoop: null,
        animOut: null,
        text: null,
        id: 'o${_next++}',
        kind: OverlayKindView.shape,
        startUs: big(atUs),
        endUs: big(atUs + 3000000),
        locked: false,
        hidden: false,
        hasAnimation: false,
        transform: const TransformView(
          x: 540,
          y: 960,
          scale: 1,
          rotation: 0,
          opacity: 1,
          flip: false,
        ),
      ),
    );
    return _overlay('shape ${shape.name} $atUs');
  }

  @override
  Future<ProjectView> addText(String content, int atUs) {
    opened!.overlays.add(
      OverlayView(
        path: null,
        keyframes: const [],
        animIn: null,
        animLoop: null,
        animOut: null,
        id: 'o${_next++}',
        kind: OverlayKindView.text,
        startUs: big(atUs),
        endUs: big(atUs + 3000000),
        locked: false,
        hidden: false,
        hasAnimation: false,
        transform: const TransformView(
          x: 540,
          y: 960,
          scale: 1,
          rotation: 0,
          opacity: 1,
          flip: false,
        ),
        text: TextElementView(
          content: content,
          font: 'Space Grotesk',
          size: 96,
          color: '#FFFFFFFF',
          uppercase: true,
          decoration: TextDecorationView.plain,
        ),
      ),
    );
    return _overlay('text $content $atUs');
  }

  @override
  Future<ProjectView> setText(String id, TextElementView text) {
    final overlays = opened!.overlays;
    final index = overlays.indexWhere((overlay) => overlay.id == id);
    final old = overlays[index];
    overlays[index] = OverlayView(
      id: old.id,
      kind: old.kind,
      startUs: old.startUs,
      endUs: old.endUs,
      locked: old.locked,
      hidden: old.hidden,
      hasAnimation: old.hasAnimation,
      transform: old.transform,
      text: text,
      animIn: old.animIn,
      animLoop: old.animLoop,
      animOut: old.animOut,
      keyframes: old.keyframes,
      path: old.path,
    );
    return _overlay(
      'set $id ${text.content.replaceAll('\n', '|')} ${text.font} '
      '${text.size.round()} ${text.color} ${text.uppercase} '
      '${text.decoration.name}',
    );
  }

  @override
  Future<ProjectView> setPresetAnim(
    String id,
    AnimSlotView slot,
    PresetKindView? kind,
    int durationUs,
  ) => _overlay('preset $id ${slot.name} ${kind?.name} $durationUs');

  /// Keyframe times per overlay, set by tests.
  final keyframeTimes = <String, List<int>>{};

  @override
  TransformView? overlayTransformAt(String id, int timeUs) =>
      const TransformView(
        x: 540,
        y: 960,
        scale: 1,
        rotation: 0,
        opacity: 1,
        flip: false,
      );

  final previewedKeys = <TransformView>[];

  @override
  void previewOverlayKey(String id, int timeUs, TransformView transform) =>
      previewedKeys.add(transform);

  @override
  Future<ProjectView> keyOverlayTransform(
    String id,
    int timeUs,
    TransformView t,
  ) => _overlay(
    'key $id $timeUs ${t.x.round()} ${t.y.round()} '
    '${t.scale.toStringAsFixed(2)} ${t.rotation.round()} ${t.opacity}',
  );

  @override
  Future<ProjectView> removeKeyframesAt(String id, int timeUs) =>
      _overlay('unkey $id $timeUs');

  @override
  Future<ProjectView> moveKeyframes(String id, int fromUs, int toUs) =>
      _overlay('rekey $id $fromUs $toUs');

  @override
  Future<ProjectView> setKeyframeEasing(
    String id,
    int timeUs,
    EasingView easing,
  ) => _overlay('ease $id $timeUs ${easing.name}');

  @override
  Future<ProjectView> setRecordedPath(String id, List<PathPointView> samples) =>
      _overlay('record $id ${samples.length}');

  @override
  Future<ProjectView> setPathOptions(
    String id, {
    required double smoothing,
    required bool orientToPath,
  }) => _overlay('path $id ${smoothing.toStringAsFixed(1)} $orientToPath');

  @override
  Future<ProjectView> movePathPoint(String id, int index, double x, double y) =>
      _overlay('point $id $index ${x.round()} ${y.round()}');

  @override
  Future<ProjectView> deletePathPoint(String id, int index) =>
      _overlay('unpoint $id $index');

  @override
  Future<ProjectView> stretchPath(String id, int lengthUs) =>
      _overlay('stretch $id $lengthUs');

  @override
  Future<ProjectView> clearPath(String id) => _overlay('clear $id');

  @override
  List<PathPointView> pathCurve(String id) => const [];

  @override
  PresetSampleView presetSample(
    PresetKindView kind,
    AnimSlotView slot,
    double progress,
  ) => PresetSampleView(
    dx: 0,
    dy: 0,
    scale: 1,
    rotation: 0,
    opacity: progress,
    reveal: 1,
  );

  @override
  Future<ProjectView> setOverlayTiming(String id, int startUs, int endUs) =>
      _overlay('timing $id $startUs $endUs');

  @override
  Future<ProjectView> shiftOverlayLayer(String id, {required bool forward}) =>
      _overlay('layer $id $forward');

  @override
  Future<ProjectView> setOverlayFlags(
    String id, {
    required bool locked,
    required bool hidden,
  }) => _overlay('flags $id $locked $hidden');

  @override
  Future<ProjectView> duplicateOverlay(String id) => _overlay('duplicate $id');

  @override
  Future<ProjectView> deleteOverlay(String id) {
    opened!.overlays.removeWhere((overlay) => overlay.id == id);
    return _overlay('delete $id');
  }

  @override
  List<OverlayBoxView> overlayBoxes(int timeUs) => boxes;

  static const library = [
    LibraryItemView(name: 'arrows-right', category: 'arrows'),
    LibraryItemView(name: 'arrows-left', category: 'arrows'),
    LibraryItemView(name: 'badges-check', category: 'badges'),
  ];

  @override
  List<LibraryItemView> libraryItems(String query, String? category) => [
    for (final item in library)
      if ((category == null || item.category == category) &&
          item.name.contains(query))
        item,
  ];

  @override
  PictureView? libraryThumbnail(String name, int size) => PictureView(
    width: 2,
    height: 2,
    rgba: Uint8List.fromList(List.filled(16, 255)),
  );

  @override
  Future<ProjectView> addLibraryItem(
    String name,
    int atUs, {
    double? x,
    double? y,
  }) {
    opened!.overlays.add(
      OverlayView(
        path: null,
        keyframes: const [],
        animIn: null,
        animLoop: null,
        animOut: null,
        text: null,
        id: 'o${_next++}',
        kind: OverlayKindView.image,
        startUs: big(atUs),
        endUs: big(atUs + 3000000),
        locked: false,
        hidden: false,
        hasAnimation: false,
        transform: TransformView(
          x: x ?? 540,
          y: y ?? 960,
          scale: 1,
          rotation: 0,
          opacity: 1,
          flip: false,
        ),
      ),
    );
    final at = x == null || y == null ? 'centre' : '${x.round()} ${y.round()}';
    return _overlay('library $name $atUs $at');
  }

  final brand = <BrandAssetView>[];

  /// Files whose name contains this are refused as too large.
  static const tooLarge = 'huge';

  @override
  Future<List<BrandAssetView>> brandKit() async => List.of(brand);

  @override
  Future<List<ImportOutcome>> importBrandAssets(List<String> paths) async => [
    for (final path in paths)
      if (path.contains(tooLarge))
        const ImportOutcome(error: ErrorCode.tooLarge)
      else
        _addBrand(path),
  ];

  ImportOutcome _addBrand(String path) {
    final name = path.split('/').last;
    brand.insert(
      0,
      BrandAssetView(
        id: 'b${_next++}',
        name: name,
        kind: name.endsWith('.gif')
            ? BrandKindView.animation
            : BrandKindView.picture,
      ),
    );
    return ImportOutcome(mediaId: brand.first.id);
  }

  @override
  Future<void> deleteBrandAsset(String id) async =>
      brand.removeWhere((asset) => asset.id == id);

  @override
  PictureView? brandThumbnail(String id, int size) =>
      libraryThumbnail(id, size);

  @override
  Future<ProjectView> addBrandAsset(
    String id,
    int atUs, {
    double? x,
    double? y,
  }) {
    final at = x == null || y == null ? 'centre' : '${x.round()} ${y.round()}';
    return _overlay('brand $id $atUs $at');
  }

  final previewed = <TransformView>[];

  @override
  void previewOverlayTransform(String id, TransformView transform) =>
      previewed.add(transform);

  @override
  Future<ProjectView> setOverlayTransform(String id, TransformView t) =>
      _overlay(
        'transform $id ${t.x.round()} ${t.y.round()} '
        '${t.scale.toStringAsFixed(2)} ${t.rotation.round()} '
        '${t.opacity} ${t.flip}',
      );

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
  Future<ProjectView> setDucking(String id, double? strength) =>
      _audio('duck $id $strength');

  @override
  Future<ProjectView> moveAudio(String id, int startUs, String? trackId) =>
      _audio('move $id $startUs $trackId');

  @override
  Future<ProjectView> trimAudio(
    String id, {
    required bool start,
    required int deltaUs,
  }) => _audio('trim $id $start $deltaUs');

  @override
  Future<ProjectView> reorderAudioTrack(String id, int to) =>
      _audio('reorder $id $to');

  @override
  Future<ProjectView> addAudioTrack() => _audio('add track');

  @override
  Future<void> startVoiceOver() async {
    audioCalls.add('record');
  }

  @override
  Future<ProjectView> stopVoiceOver(int startUs) => _audio('stop $startUs');

  @override
  double get voiceLevel => 0.5;

  @override
  Future<ProjectView> removeAudioTrack(String id) => _audio('remove $id');

  @override
  Future<ProjectView> renameAudioTrack(String id, String name) =>
      _audio('rename $id $name');

  @override
  Future<ProjectView> setTrackFlags(
    TrackView track, {
    bool? muted,
    bool? locked,
    bool? hidden,
  }) => _audio('flags ${track.kind.name} $muted $locked $hidden');

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
  Future<ThumbnailView?> cover(String projectId) async => null;

  @override
  int exportSizeEstimate(int width, int fps, int durationUs) => 5 * 1000 * 1000;

  var modelReady = false;
  final speechEvents = StreamController<TaskEvent>.broadcast();
  final speechCalls = <String>[];

  @override
  ModelView speechModel() =>
      ModelView(ready: modelReady, bytes: BigInt.from(60000000));

  @override
  Stream<TaskEvent> downloadSpeechModel() {
    speechCalls.add('download');
    return speechEvents.stream;
  }

  @override
  Future<void> deleteSpeechModel() async {
    speechCalls.add('delete');
    modelReady = false;
  }

  @override
  Stream<TaskEvent> transcribe() {
    speechCalls.add('transcribe');
    return speechEvents.stream;
  }

  @override
  Future<ProjectView> editWord(
    TranscriptView transcript,
    String segment,
    int word,
    String text,
  ) async {
    speechCalls.add('edit $segment $word $text');
    return view();
  }

  @override
  Future<ProjectView> toggleEmphasis(
    TranscriptView transcript,
    String segment,
    int word,
  ) async {
    speechCalls.add('emphasis $segment $word');
    return view();
  }

  @override
  Future<ProjectView> setCaptionPreset(CaptionPresetView preset) async {
    speechCalls.add('preset ${preset.name}');
    return view();
  }

  @override
  Future<ProjectView> setCaptionsEnabled({required bool enabled}) async {
    speechCalls.add('enabled $enabled');
    return view();
  }

  @override
  Future<ProjectView> setCaptionAnimation(
    CaptionAnimationView animation,
  ) async {
    speechCalls.add('animation ${animation.name}');
    return view();
  }

  @override
  Future<ProjectView> setCaptionStyle({
    required String font,
    required double size,
    required bool uppercase,
    required String color,
    required String emphasisColor,
    required double anchor,
  }) async {
    speechCalls.add('style $font $uppercase');
    return view();
  }

  final srts = <String>[];

  @override
  Future<void> writeSrt(String path) async => srts.add(path);

  @override
  Future<ProjectView> hideSegment(
    TranscriptView transcript,
    String segment, {
    required bool hidden,
  }) async {
    speechCalls.add('hide $segment $hidden');
    return view();
  }

  var collected = 0;

  @override
  Future<ProjectView> collectMedia() async {
    collected++;
    return view();
  }

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
