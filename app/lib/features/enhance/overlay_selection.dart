import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../design_system/tokens.g.dart';
import '../project/project_providers.dart';

/// The canvas every overlay position is given in (docs/06).
const canvasWidth = 1080.0;
const canvasHeight = 1920.0;

const _dash = 6.0;
const _gap = 4.0;
const _handleRadius = 5.0;
const _rotateHandleReach = 24.0;

/// Whether `point`, in canvas units, falls inside the turned box.
bool boxContains(OverlayBoxView box, Offset point) {
  final turn = -box.rotation * math.pi / 180;
  final dx = point.dx - box.centerX;
  final dy = point.dy - box.centerY;
  final x = dx * math.cos(turn) - dy * math.sin(turn);
  final y = dx * math.sin(turn) + dy * math.cos(turn);
  return x.abs() <= box.width / 2 && y.abs() <= box.height / 2;
}

/// The selection drawn over the preview, from where the engine draws the element: Flutter never
/// guesses a position (T-0701). A tap picks the topmost element under the finger.
class OverlaySelection extends ConsumerWidget {
  const OverlaySelection({
    super.key,
    required this.projectId,
    required this.playheadUs,
  });

  final String projectId;
  final ValueNotifier<int> playheadUs;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Rebuilt after every edit, so a moved or deleted element is redrawn at once.
    ref.watch(editorProvider(projectId));
    final selected = ref.watch(selectedOverlayProvider);
    final gateway = ref.read(projectGatewayProvider);
    return LayoutBuilder(
      builder: (context, box) {
        final scale = box.maxWidth / canvasWidth;
        return GestureDetector(
          behavior: HitTestBehavior.translucent,
          onTapUp: (details) {
            final point = details.localPosition / scale;
            final boxes = gateway.overlayBoxes(playheadUs.value);
            String? hit;
            for (final candidate in boxes) {
              if (boxContains(candidate, point)) {
                hit = candidate.id;
                break;
              }
            }
            ref.read(selectedOverlayProvider.notifier).set(hit);
          },
          child: ValueListenableBuilder(
            valueListenable: playheadUs,
            builder: (context, time, _) {
              OverlayBoxView? shown;
              if (selected != null) {
                for (final candidate in gateway.overlayBoxes(time)) {
                  if (candidate.id == selected) shown = candidate;
                }
              }
              return CustomPaint(
                size: Size.infinite,
                painter: SelectionPainter(box: shown, scale: scale),
              );
            },
          ),
        );
      },
    );
  }
}

/// Dashed violet box, corner handles for scale and a top handle for rotation (docs/04 S40).
class SelectionPainter extends CustomPainter {
  SelectionPainter({required this.box, required this.scale});

  final OverlayBoxView? box;
  final double scale;

  @override
  void paint(Canvas canvas, Size size) {
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
    _dashed(canvas, rect, line);
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

  void _dashed(Canvas canvas, Rect rect, Paint paint) {
    final corners = [
      rect.topLeft,
      rect.topRight,
      rect.bottomRight,
      rect.bottomLeft,
      rect.topLeft,
    ];
    for (var i = 0; i < 4; i++) {
      final from = corners[i];
      final to = corners[i + 1];
      final length = (to - from).distance;
      if (length == 0) continue;
      final step = (to - from) / length;
      for (var at = 0.0; at < length; at += _dash + _gap) {
        final end = math.min(at + _dash, length);
        canvas.drawLine(from + step * at, from + step * end, paint);
      }
    }
  }

  @override
  bool shouldRepaint(SelectionPainter old) =>
      old.scale != scale ||
      old.box?.id != box?.id ||
      old.box?.centerX != box?.centerX ||
      old.box?.centerY != box?.centerY ||
      old.box?.width != box?.width ||
      old.box?.height != box?.height ||
      old.box?.rotation != box?.rotation;
}
