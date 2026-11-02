import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
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
  _Gesture(this.id, this.mode, this.start, this.box, this.origin);

  final String id;
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

class _OverlaySelectionState extends ConsumerState<OverlaySelection> {
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

  @override
  void initState() {
    super.initState();
    _wordsFocus.addListener(() {
      if (!_wordsFocus.hasFocus) _finishTyping();
    });
  }

  @override
  void dispose() {
    _words.dispose();
    _wordsFocus.dispose();
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
    if (last != null &&
        now - last.$1 <= kDoubleTapTimeout &&
        (details.localPosition - last.$2).distance <= kDoubleTapSlop) {
      _lastTap = null;
      _doubleTapAt = details.localPosition;
      _startTyping();
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
    ref.read(selectedOverlayProvider.notifier).set(hit);
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
    _gesture = _Gesture(target.id, mode, start, target, point);
  }

  void _update(ScaleUpdateDetails details) {
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
    }
    setState(() {
      _shown = next;
      _guides = guides;
    });
  }

  Future<void> _end(ScaleEndDetails details) async {
    final gesture = _gesture;
    final shown = _shown;
    _gesture = null;
    setState(() {
      _shown = null;
      _guides = const {};
    });
    if (gesture == null || shown == null) return;
    final l10n = AppLocalizations.of(context);
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
    final gateway = ref.read(projectGatewayProvider);
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
              onScaleStart: _start,
              onScaleUpdate: _update,
              onScaleEnd: _end,
              child: ValueListenableBuilder(
                valueListenable: widget.playheadUs,
                builder: (context, time, _) {
                  OverlayBoxView? shown;
                  OverlayBoxView? typing;
                  for (final candidate in gateway.overlayBoxes(time)) {
                    if (candidate.id == selected) shown = candidate;
                    if (candidate.id == _editing) typing = candidate;
                  }
                  final selection = CustomPaint(
                    size: Size.infinite,
                    painter: SelectionPainter(
                      box: shown,
                      scale: _scale,
                      guides: _guides,
                    ),
                  );
                  if (typing == null) return selection;
                  return Stack(
                    fit: StackFit.expand,
                    children: [selection, _typing(typing)],
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
  });

  final OverlayBoxView? box;
  final double scale;
  final Set<Guide> guides;

  @override
  void paint(Canvas canvas, Size size) {
    _paintGuides(canvas, size);
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
      old.scale != scale ||
      old.guides != guides ||
      old.box?.id != box?.id ||
      old.box?.centerX != box?.centerX ||
      old.box?.centerY != box?.centerY ||
      old.box?.width != box?.width ||
      old.box?.height != box?.height ||
      old.box?.rotation != box?.rotation;
}
