import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../bridge/api/projects.dart';
import '../../bridge/api/transcribe.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';
import 'library_section.dart';
import 'text_section.dart';

/// The canvas every overlay position is given in (docs/06).
const canvasWidth = 1080.0;
const canvasHeight = 1920.0;

/// The platforms' interfaces cover the frame above and below these lines (FR-CAP-05).
const safeTop = canvasHeight * 0.15;
const safeBottom = canvasHeight * 0.75;

const _dash = 6.0;
const _gap = 4.0;
const _handleRadius = 5.0;
const _rotateHandleReach = 24.0;

/// How close a handle must be touched, and how near a guide pulls, in screen pixels.
const _touchSlop = 14.0;
const _snapPixels = 8.0;

enum Guide { centreX, centreY, safeTop, safeBottom }

/// Whether `point`, in canvas units, falls inside the turned box.
bool boxContains(OverlayBoxView box, Offset point) {
  final local = _toBox(box, point);
  return local.dx.abs() <= box.width / 2 && local.dy.abs() <= box.height / 2;
}

/// `point` in the box's own frame: origin at its centre, turned with it.
Offset _toBox(OverlayBoxView box, Offset point) {
  final turn = -box.rotation * math.pi / 180;
  final dx = point.dx - box.centerX;
  final dy = point.dy - box.centerY;
  return Offset(
    dx * math.cos(turn) - dy * math.sin(turn),
    dx * math.sin(turn) + dy * math.cos(turn),
  );
}

/// Pulls a centre onto the middle lines, and the box's edges onto the safe zone, when within
/// `threshold` canvas units. Returns the guides it snapped to, for drawing.
({Offset centre, Set<Guide> guides}) snapCentre(
  Offset centre,
  Size size,
  double threshold,
) {
  var x = centre.dx;
  var y = centre.dy;
  final guides = <Guide>{};
  if ((x - canvasWidth / 2).abs() <= threshold) {
    x = canvasWidth / 2;
    guides.add(Guide.centreX);
  }
  final top = y - size.height / 2;
  final bottom = y + size.height / 2;
  if ((y - canvasHeight / 2).abs() <= threshold) {
    y = canvasHeight / 2;
    guides.add(Guide.centreY);
  } else if ((top - safeTop).abs() <= threshold) {
    y = safeTop + size.height / 2;
    guides.add(Guide.safeTop);
  } else if ((bottom - safeBottom).abs() <= threshold) {
    y = safeBottom - size.height / 2;
    guides.add(Guide.safeBottom);
  }
  return (centre: Offset(x, y), guides: guides);
}

enum _Mode { move, scale, rotate }

class _Gesture {
  _Gesture(
    this.id,
    this.mode,
    this.start,
    this.box,
    this.origin, {
    this.group = const {},
  });

  final String id;

  /// The others moving with it, and where each one started.
  final Map<String, TransformView> group;
  final _Mode mode;
  final TransformView start;
  final OverlayBoxView box;

  /// Where the finger went down, in canvas units.
  final Offset origin;
}

TransformView _with(
  TransformView base, {
  double? x,
  double? y,
  double? scale,
  double? rotation,
}) => TransformView(
  x: x ?? base.x,
  y: y ?? base.y,
  scale: scale ?? base.scale,
  rotation: rotation ?? base.rotation,
  opacity: base.opacity,
  flip: base.flip,
);

/// The selection drawn over the preview, from where the engine draws the element: Flutter never
/// guesses a position (T-0701). A tap picks the topmost element; dragging moves it, a pinch
/// scales and turns it, the corner handles scale and the top handle turns (T-0702). Each
/// gesture shows live and is recorded once, when it ends.
class OverlaySelection extends ConsumerStatefulWidget {
  const OverlaySelection({
    super.key,
    required this.projectId,
    required this.playheadUs,
  });

  final String projectId;
  final ValueNotifier<int> playheadUs;

  @override
  ConsumerState<OverlaySelection> createState() => _OverlaySelectionState();
}

class _OverlaySelectionState extends ConsumerState<OverlaySelection>
    with SingleTickerProviderStateMixin {
  // Path mode (T-0708): recording follows the finger at each frame of a slowed playback.
  Ticker? _recorder;
  final _recorded = <PathPointView>[];
  Offset? _finger;
  int? _pointDrag;
  Offset? _pointAt;

  /// The selected element, when its path is being edited.
  String? get _pathId {
    final id = ref.read(pathEditingProvider);
    return id != null && id == ref.read(selectedOverlayProvider) ? id : null;
  }

  int? _pointNear(OverlayView overlay, Offset point) {
    final points = overlay.path?.points ?? const <PathPointView>[];
    final slop = _touchSlop / _scale;
    for (final (index, candidate) in points.indexed) {
      if ((Offset(candidate.x, candidate.y) - point).distance <= slop) {
        return index;
      }
    }
    return null;
  }

  void _beginRecording(OverlayView overlay, Offset point, double rate) {
    final transport = ref.read(previewTransportProvider);
    final gateway = ref.read(projectGatewayProvider);
    _recorded.clear();
    _finger = point;
    final end = overlay.endUs.toInt();
    transport.playAt(rate);
    _recorder = createTicker((_) {
      final finger = _finger;
      if (finger == null) return;
      final time = transport.positionUs;
      _recorded.add(PathPointView(timeUs: time, x: finger.dx, y: finger.dy));
      final base = overlay.transform;
      gateway.previewOverlayTransform(
        overlay.id,
        TransformView(
          x: finger.dx,
          y: finger.dy,
          scale: base.scale,
          rotation: base.rotation,
          opacity: base.opacity,
          flip: base.flip,
        ),
      );
      setState(() {});
      if (time >= end || !transport.isPlaying) _finishRecording(overlay.id);
    })..start();
  }

  Future<void> _finishRecording(String id) async {
    final recorder = _recorder;
    if (recorder == null) return;
    _recorder = null;
    recorder.dispose();
    ref.read(previewTransportProvider).pause();
    ref.read(recordingPathProvider.notifier).set(null);
    final samples = List.of(_recorded);
    setState(_recorded.clear);
    if (samples.length < 2) return;
    final l10n = AppLocalizations.of(context);
    try {
      await ref
          .read(editorProvider(widget.projectId).notifier)
          .apply((gateway) => gateway.setRecordedPath(id, samples));
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  Future<void> _applyPath(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  ) async {
    final l10n = AppLocalizations.of(context);
    try {
      await ref.read(editorProvider(widget.projectId).notifier).apply(action);
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  double _scale = 1;
  _Gesture? _gesture;
  TransformView? _shown;
  Set<Guide> _guides = const {};

  /// Where the finger went down: the gesture only starts once it has moved a little, and a
  /// small handle must be judged from the first touch.
  Offset? _down;

  /// A text element being typed into on the preview (T-0705).
  String? _editing;
  Offset? _doubleTapAt;

  /// The last tap, to spot a second one: a double-tap recogniser would hold every single tap.
  (Duration, Offset)? _lastTap;
  final _words = TextEditingController();
  final _wordsFocus = FocusNode();

  /// A caption word tapped once: its emphasis changes unless a second tap edits it (T-1202).
  Timer? _captionTap;
  CaptionHitView? _captionEditing;
  final _captionWord = TextEditingController();
  final _captionFocus = FocusNode();

  @override
  void initState() {
    super.initState();
    _wordsFocus.addListener(() {
      if (!_wordsFocus.hasFocus) _finishTyping();
    });
    _captionFocus.addListener(() {
      if (!_captionFocus.hasFocus) _finishCaption();
    });
  }

  @override
  void dispose() {
    _recorder?.dispose();
    _words.dispose();
    _wordsFocus.dispose();
    _captionTap?.cancel();
    _captionWord.dispose();
    _captionFocus.dispose();
    super.dispose();
  }

  void _startTyping() {
    final at = _doubleTapAt;
    if (at == null) return;
    final point = at / _scale;
    for (final box in _boxes) {
      if (!boxContains(box, point)) continue;
      final overlay = _overlay(box.id);
      final text = overlay?.text;
      if (overlay == null || text == null || overlay.locked) return;
      ref.read(selectedOverlayProvider.notifier).set(box.id);
      setState(() {
        _editing = box.id;
        _words.text = text.content;
      });
      _wordsFocus.requestFocus();
      return;
    }
  }

  Future<void> _finishTyping() async {
    final id = _editing;
    if (id == null) return;
    setState(() => _editing = null);
    final text = _overlay(id)?.text;
    final words = _words.text;
    if (text == null || words.trim().isEmpty || words == text.content) return;
    final l10n = AppLocalizations.of(context);
    try {
      await ref
          .read(editorProvider(widget.projectId).notifier)
          .apply(
            (gateway) => gateway.setText(id, copyText(text, content: words)),
          );
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  Widget _typing(OverlayBoxView box) {
    final width = math.max(box.width * _scale, 160.0);
    return Positioned(
      left: box.centerX * _scale - width / 2,
      top: (box.centerY - box.height / 2) * _scale,
      width: width,
      child: Material(
        color: ShortedColors.surface.withValues(alpha: 0.85),
        borderRadius: BorderRadius.circular(ShortedRadius.s),
        child: TextField(
          key: const ValueKey('preview-text-editor'),
          controller: _words,
          focusNode: _wordsFocus,
          textAlign: TextAlign.center,
          minLines: 1,
          maxLines: 6,
          keyboardType: TextInputType.multiline,
          decoration: const InputDecoration(
            isDense: true,
            border: InputBorder.none,
            contentPadding: EdgeInsets.all(ShortedSpace.s),
          ),
          onTapOutside: (_) => _wordsFocus.unfocus(),
        ),
      ),
    );
  }

  OverlayView? _overlay(String id) {
    final overlays =
        ref.read(editorProvider(widget.projectId)).value?.view.overlays ??
        const <OverlayView>[];
    for (final overlay in overlays) {
      if (overlay.id == id) return overlay;
    }
    return null;
  }

  List<OverlayBoxView> get _boxes =>
      ref.read(projectGatewayProvider).overlayBoxes(widget.playheadUs.value);

  void _tap(TapUpDetails details) {
    final now = SchedulerBinding.instance.currentSystemFrameTimeStamp;
    final last = _lastTap;
    _lastTap = (now, details.localPosition);
    final twice =
        last != null &&
        now - last.$1 <= kDoubleTapTimeout &&
        (details.localPosition - last.$2).distance <= kDoubleTapSlop;
    final pathId = _pathId;
    if (pathId != null) {
      // On a path, a double tap deletes the point under it; nothing else selects.
      final overlay = _overlay(pathId);
      final index = overlay == null
          ? null
          : _pointNear(overlay, details.localPosition / _scale);
      if (twice && index != null) {
        _lastTap = null;
        _applyPath((gateway) => gateway.deletePathPoint(pathId, index));
      }
      return;
    }
    final point = details.localPosition / _scale;
    String? hit;
    for (final candidate in _boxes) {
      if (boxContains(candidate, point)) {
        hit = candidate.id;
        break;
      }
    }
    if (twice) {
      _lastTap = null;
      _doubleTapAt = details.localPosition;
      if (hit != null) {
        _startTyping();
      } else {
        _captionTap?.cancel();
        unawaited(_editCaption(point));
      }
      return;
    }
    final current = ref.read(selectedOverlayProvider);
    if (hit != null && HardwareKeyboard.instance.isShiftPressed) {
      ref.read(overlayGroupProvider.notifier).toggle(hit, current);
    } else {
      ref.read(overlayGroupProvider.notifier).clear();
    }
    ref.read(selectedOverlayProvider.notifier).set(hit);
    _captionTap?.cancel();
    if (hit == null) {
      _captionTap = Timer(kDoubleTapTimeout, () => _emphasise(point));
    }
  }

  /// A long press adds an element to the selection, or takes it out (T-1206).
  void _longPress(LongPressStartDetails details) {
    final point = details.localPosition / _scale;
    for (final candidate in _boxes) {
      if (!boxContains(candidate, point)) continue;
      HapticFeedback.selectionClick();
      ref
          .read(overlayGroupProvider.notifier)
          .toggle(candidate.id, ref.read(selectedOverlayProvider));
      ref.read(selectedOverlayProvider.notifier).set(candidate.id);
      return;
    }
  }

  Future<CaptionHitView?> _captionAt(Offset point) => ref
      .read(projectGatewayProvider)
      .captionWordAt(
        widget.playheadUs.value,
        point.dx / canvasWidth,
        point.dy / canvasHeight,
      );

  TranscriptView _owner(CaptionHitView word) => TranscriptView(
    clipId: word.clipId,
    audioId: word.audioId,
    language: '',
    segments: const [],
  );

  /// One tap on a caption word turns its emphasis on or off (S30).
  Future<void> _emphasise(Offset point) async {
    final word = await _captionAt(point);
    if (word == null || !mounted) return;
    await _applyPath(
      (gateway) =>
          gateway.toggleEmphasis(_owner(word), word.segment, word.word),
    );
  }

  /// Two taps on a caption word open it for correction, in place.
  Future<void> _editCaption(Offset point) async {
    final word = await _captionAt(point);
    if (word == null || !mounted) return;
    ref.read(previewTransportProvider).pause();
    setState(() {
      _captionEditing = word;
      _captionWord.text = word.text;
    });
    _captionFocus.requestFocus();
  }

  Future<void> _finishCaption() async {
    final word = _captionEditing;
    if (word == null) return;
    final text = _captionWord.text.trim();
    setState(() => _captionEditing = null);
    if (text.isEmpty || text == word.text) return;
    await _applyPath(
      (gateway) =>
          gateway.editWord(_owner(word), word.segment, word.word, text),
    );
  }

  Widget _captionEditor(CaptionHitView word, Size size) {
    final width = math.max(word.width * size.width + 2 * ShortedSpace.l, 140.0);
    final centre = (word.left + word.width / 2) * size.width;
    return Positioned(
      left: (centre - width / 2).clamp(0, math.max(0, size.width - width)),
      top: word.top * size.height,
      width: width,
      child: Material(
        color: ShortedColors.surface.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(ShortedRadius.s),
        child: TextField(
          key: const ValueKey('preview-caption-editor'),
          controller: _captionWord,
          focusNode: _captionFocus,
          textAlign: TextAlign.center,
          decoration: const InputDecoration(
            isDense: true,
            border: InputBorder.none,
            contentPadding: EdgeInsets.all(ShortedSpace.s),
          ),
          onSubmitted: (_) => _captionFocus.unfocus(),
          onTapOutside: (_) => _captionFocus.unfocus(),
        ),
      ),
    );
  }

  /// Which handle of the selected box is under `point`, if any.
  _Mode? _handleAt(OverlayBoxView box, Offset point) {
    final local = _toBox(box, point);
    final slop = _touchSlop / _scale;
    final halfWidth = box.width / 2;
    final halfHeight = box.height / 2;
    for (final corner in [
      Offset(-halfWidth, -halfHeight),
      Offset(halfWidth, -halfHeight),
      Offset(-halfWidth, halfHeight),
      Offset(halfWidth, halfHeight),
    ]) {
      if ((local - corner).distance <= slop) return _Mode.scale;
    }
    final knob = Offset(0, -halfHeight - _rotateHandleReach / _scale);
    if ((local - knob).distance <= slop) return _Mode.rotate;
    return null;
  }

  void _start(ScaleStartDetails details) {
    final point = (_down ?? details.localFocalPoint) / _scale;
    final pathId = _pathId;
    if (pathId != null) {
      final overlay = _overlay(pathId);
      if (overlay == null) return;
      final rate = ref.read(recordingPathProvider);
      if (rate != null) {
        _beginRecording(overlay, point, rate);
      } else {
        _pointDrag = _pointNear(overlay, point);
      }
      return;
    }
    final boxes = _boxes;
    final selected = ref.read(selectedOverlayProvider);
    OverlayBoxView? target;
    _Mode? mode;
    for (final box in boxes) {
      if (box.id == selected) {
        mode = _handleAt(box, point);
        if (mode != null) target = box;
      }
    }
    if (target == null) {
      for (final box in boxes) {
        if (boxContains(box, point)) {
          target = box;
          mode = _Mode.move;
          break;
        }
      }
    }
    if (target == null || mode == null) return;
    final overlay = _overlay(target.id);
    if (overlay == null || overlay.locked) return;
    if (target.id != selected) {
      ref.read(selectedOverlayProvider.notifier).set(target.id);
    }
    // In keyframe mode the gesture starts from what is keyed at the playhead.
    final start = ref.read(keyframeModeProvider)
        ? ref
                  .read(projectGatewayProvider)
                  .overlayTransformAt(target.id, widget.playheadUs.value) ??
              overlay.transform
        : overlay.transform;
    final group = ref.read(overlayGroupProvider);
    final together =
        mode == _Mode.move &&
        group.contains(target.id) &&
        !ref.read(keyframeModeProvider);
    _gesture = _Gesture(
      target.id,
      mode,
      start,
      target,
      point,
      group: {
        if (together)
          for (final other in group.map(_overlay).nonNulls)
            if (other.id != target.id && !other.locked)
              other.id: other.transform,
      },
    );
  }

  void _update(ScaleUpdateDetails details) {
    if (_recorder != null) {
      _finger = details.localFocalPoint / _scale;
      return;
    }
    if (_pointDrag != null) {
      setState(() => _pointAt = details.localFocalPoint / _scale);
      return;
    }
    final gesture = _gesture;
    if (gesture == null) return;
    final point = details.localFocalPoint / _scale;
    final start = gesture.start;
    final box = gesture.box;
    final centre = Offset(box.centerX, box.centerY);
    var guides = const <Guide>{};
    final TransformView next;
    switch (gesture.mode) {
      case _Mode.move:
        final delta = point - gesture.origin;
        final scale = start.scale * details.scale;
        final grown = scale / start.scale;
        final snapped = snapCentre(
          Offset(start.x, start.y) + delta,
          Size(box.width * grown, box.height * grown),
          _snapPixels / _scale,
        );
        guides = snapped.guides;
        next = _with(
          start,
          x: snapped.centre.dx,
          y: snapped.centre.dy,
          scale: scale,
          rotation: start.rotation + details.rotation * 180 / math.pi,
        );
      case _Mode.scale:
        final from = (gesture.origin - centre).distance;
        final ratio = from == 0 ? 1.0 : (point - centre).distance / from;
        next = _with(start, scale: start.scale * ratio);
      case _Mode.rotate:
        final before = gesture.origin - centre;
        final after = point - centre;
        final turn = after.direction - before.direction;
        next = _with(start, rotation: start.rotation + turn * 180 / math.pi);
    }
    if (guides.difference(_guides).isNotEmpty) {
      HapticFeedback.selectionClick();
    }
    final gateway = ref.read(projectGatewayProvider);
    if (ref.read(keyframeModeProvider)) {
      gateway.previewOverlayKey(gesture.id, widget.playheadUs.value, next);
    } else {
      gateway.previewOverlayTransform(gesture.id, next);
      final dx = next.x - start.x;
      final dy = next.y - start.y;
      for (final MapEntry(key: id, value: from) in gesture.group.entries) {
        gateway.previewOverlayTransform(
          id,
          _with(from, x: from.x + dx, y: from.y + dy),
        );
      }
    }
    setState(() {
      _shown = next;
      _guides = guides;
    });
  }

  Future<void> _end(ScaleEndDetails details) async {
    final pathId = _pathId;
    if (_recorder != null && pathId != null) {
      await _finishRecording(pathId);
      return;
    }
    final dragged = _pointDrag;
    final at = _pointAt;
    if (dragged != null) {
      setState(() {
        _pointDrag = null;
        _pointAt = null;
      });
      if (pathId != null && at != null) {
        await _applyPath(
          (gateway) => gateway.movePathPoint(pathId, dragged, at.dx, at.dy),
        );
      }
      return;
    }
    final gesture = _gesture;
    final shown = _shown;
    _gesture = null;
    setState(() {
      _shown = null;
      _guides = const {};
    });
    if (gesture == null || shown == null) return;
    final l10n = AppLocalizations.of(context);
    if (gesture.group.isNotEmpty) {
      final ids = [gesture.id, ...gesture.group.keys];
      final dx = shown.x - gesture.start.x;
      final dy = shown.y - gesture.start.y;
      await _applyPath((gateway) => gateway.moveOverlays(ids, dx, dy));
      return;
    }
    try {
      final keyed = ref.read(keyframeModeProvider);
      final atUs = widget.playheadUs.value;
      await ref
          .read(editorProvider(widget.projectId).notifier)
          .apply(
            (gateway) => keyed
                ? gateway.keyOverlayTransform(gesture.id, atUs, shown)
                : gateway.setOverlayTransform(gesture.id, shown),
          );
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  @override
  Widget build(BuildContext context) {
    // Rebuilt after every edit, so a moved or deleted element is redrawn at once.
    ref.watch(editorProvider(widget.projectId));
    final selected = ref.watch(selectedOverlayProvider);
    final group = ref.watch(overlayGroupProvider);
    final editingPath = ref.watch(pathEditingProvider);
    ref.watch(recordingPathProvider);
    final gateway = ref.read(projectGatewayProvider);
    final pathOverlay = editingPath != null && editingPath == selected
        ? _overlay(editingPath)
        : null;
    final pathPoints = [
      for (final (index, point)
          in (pathOverlay?.path?.points ?? const <PathPointView>[]).indexed)
        index == _pointDrag && _pointAt != null
            ? _pointAt!
            : Offset(point.x, point.y),
    ];
    final curve = _recorder != null
        ? [for (final point in _recorded) Offset(point.x, point.y)]
        : pathOverlay == null
        ? const <Offset>[]
        : [
            for (final point in gateway.pathCurve(pathOverlay.id))
              Offset(point.x, point.y),
          ];
    return LayoutBuilder(
      builder: (context, box) {
        _scale = box.maxWidth / canvasWidth;
        return DragTarget<LibraryDrag>(
          onAcceptWithDetails: (details) {
            final area = context.findRenderObject()! as RenderBox;
            final point = area.globalToLocal(details.offset) / _scale;
            placeLibraryItem(
              ref,
              context,
              widget.projectId,
              details.data,
              x: point.dx,
              y: point.dy,
            );
          },
          builder: (context, candidates, _) => Listener(
            onPointerDown: (event) => _down = event.localPosition,
            child: GestureDetector(
              behavior: HitTestBehavior.translucent,
              onTapUp: _tap,
              onLongPressStart: _longPress,
              onScaleStart: _start,
              onScaleUpdate: _update,
              onScaleEnd: _end,
              child: ValueListenableBuilder(
                valueListenable: widget.playheadUs,
                builder: (context, time, _) {
                  OverlayBoxView? shown;
                  OverlayBoxView? typing;
                  final grouped = <OverlayBoxView>[];
                  for (final candidate in gateway.overlayBoxes(time)) {
                    if (candidate.id == selected) shown = candidate;
                    if (candidate.id != selected &&
                        group.contains(candidate.id)) {
                      grouped.add(candidate);
                    }
                    if (candidate.id == _editing) typing = candidate;
                  }
                  final selection = CustomPaint(
                    size: Size.infinite,
                    painter: SelectionPainter(
                      box: shown,
                      scale: _scale,
                      guides: _guides,
                      curve: curve,
                      points: _recorder == null ? pathPoints : const [],
                      others: grouped,
                    ),
                  );
                  final caption = _captionEditing;
                  if (typing == null && caption == null) return selection;
                  return Stack(
                    fit: StackFit.expand,
                    children: [
                      selection,
                      if (typing != null) _typing(typing),
                      if (caption != null) _captionEditor(caption, box.biggest),
                    ],
                  );
                },
              ),
            ),
          ),
        );
      },
    );
  }
}

/// Dashed violet box, corner handles for scale and a top handle for rotation (docs/04 S40), and
/// the guides the element snapped to.
class SelectionPainter extends CustomPainter {
  SelectionPainter({
    required this.box,
    required this.scale,
    this.guides = const {},
    this.curve = const [],
    this.points = const [],
    this.others = const [],
  });

  final OverlayBoxView? box;

  /// The other elements of a group selection, outlined.
  final List<OverlayBoxView> others;
  final double scale;
  final Set<Guide> guides;

  /// A recorded path in canvas units: the curve the element follows, and its points.
  final List<Offset> curve;
  final List<Offset> points;

  @override
  void paint(Canvas canvas, Size size) {
    _paintGuides(canvas, size);
    _paintPath(canvas);
    final outline = Paint()
      ..color = ShortedColors.overlay
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    for (final other in others) {
      canvas.save();
      canvas.translate(other.centerX * scale, other.centerY * scale);
      canvas.rotate(other.rotation * math.pi / 180);
      canvas.drawRect(
        Rect.fromCenter(
          center: Offset.zero,
          width: other.width * scale,
          height: other.height * scale,
        ),
        outline,
      );
      canvas.restore();
    }
    final shown = box;
    if (shown == null) return;
    final width = shown.width * scale;
    final height = shown.height * scale;
    canvas.save();
    canvas.translate(shown.centerX * scale, shown.centerY * scale);
    canvas.rotate(shown.rotation * math.pi / 180);
    final rect = Rect.fromCenter(
      center: Offset.zero,
      width: width,
      height: height,
    );
    final line = Paint()
      ..color = ShortedColors.overlay
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5;
    _dashed(canvas, [rect.topLeft, rect.topRight], line);
    _dashed(canvas, [rect.topRight, rect.bottomRight], line);
    _dashed(canvas, [rect.bottomRight, rect.bottomLeft], line);
    _dashed(canvas, [rect.bottomLeft, rect.topLeft], line);
    final handle = Paint()..color = ShortedColors.overlay;
    for (final corner in [
      rect.topLeft,
      rect.topRight,
      rect.bottomLeft,
      rect.bottomRight,
    ]) {
      canvas.drawCircle(corner, _handleRadius, handle);
    }
    final top = rect.topCenter;
    final knob = top.translate(0, -_rotateHandleReach);
    canvas.drawLine(top, knob, line);
    canvas.drawCircle(knob, _handleRadius, handle);
    canvas.restore();
  }

  /// The lime trail of S40, with a handle on each kept point.
  void _paintPath(Canvas canvas) {
    if (curve.length > 1) {
      final trail = Path()
        ..moveTo(curve.first.dx * scale, curve.first.dy * scale);
      for (final point in curve.skip(1)) {
        trail.lineTo(point.dx * scale, point.dy * scale);
      }
      canvas.drawPath(
        trail,
        Paint()
          ..color = ShortedColors.accent
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2.5
          ..strokeJoin = StrokeJoin.round,
      );
    }
    for (final point in points) {
      final at = point * scale;
      canvas.drawCircle(
        at,
        _handleRadius + 1,
        Paint()..color = ShortedColors.bg,
      );
      canvas.drawCircle(
        at,
        _handleRadius,
        Paint()..color = ShortedColors.accent,
      );
    }
  }

  void _paintGuides(Canvas canvas, Size size) {
    final line = Paint()
      ..color = ShortedColors.accent
      ..strokeWidth = 1;
    for (final guide in guides) {
      final (from, to) = switch (guide) {
        Guide.centreX => (
          Offset(canvasWidth / 2 * scale, 0),
          Offset(canvasWidth / 2 * scale, size.height),
        ),
        Guide.centreY => (
          Offset(0, canvasHeight / 2 * scale),
          Offset(size.width, canvasHeight / 2 * scale),
        ),
        Guide.safeTop => (
          Offset(0, safeTop * scale),
          Offset(size.width, safeTop * scale),
        ),
        Guide.safeBottom => (
          Offset(0, safeBottom * scale),
          Offset(size.width, safeBottom * scale),
        ),
      };
      _dashed(canvas, [from, to], line);
    }
  }

  void _dashed(Canvas canvas, List<Offset> ends, Paint paint) {
    final [from, to] = ends;
    final length = (to - from).distance;
    if (length == 0) return;
    final step = (to - from) / length;
    for (var at = 0.0; at < length; at += _dash + _gap) {
      final end = math.min(at + _dash, length);
      canvas.drawLine(from + step * at, from + step * end, paint);
    }
  }

  @override
  bool shouldRepaint(SelectionPainter old) =>
      old.others.length != others.length ||
      old.scale != scale ||
      old.guides != guides ||
      old.curve != curve ||
      old.points != points ||
      old.box?.id != box?.id ||
      old.box?.centerX != box?.centerX ||
      old.box?.centerY != box?.centerY ||
      old.box?.width != box?.width ||
      old.box?.height != box?.height ||
      old.box?.rotation != box?.rotation;
}
