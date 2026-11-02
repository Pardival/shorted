import 'package:flutter/material.dart';

import '../../design_system/tokens.g.dart';
import 'timeline_layout.dart';

const _itemRadius = Radius.circular(ShortedRadius.s);
const _itemGap = 2.0;

Color _trackColor(TrackKind kind) => switch (kind) {
  TrackKind.widgets => ShortedColors.overlay,
  TrackKind.video => ShortedColors.mediaPlaceholder,
  TrackKind.captions || TrackKind.voiceOver => ShortedColors.accent,
  TrackKind.audio => ShortedColors.lineStrong,
};

/// Everything but the playhead, which has its own layer: scrubbing never repaints this.
class TimelinePainter extends CustomPainter {
  TimelinePainter({
    required this.layout,
    required this.tracks,
    required this.durationUs,
    required this.selectedId,
    required this.textDirection,
    this.dragPreview,
  });

  final TimelineLayout layout;
  final List<TimelineTrack> tracks;
  final int durationUs;
  final String? selectedId;
  final TextDirection textDirection;

  /// A range being dragged, drawn over its track until the edit is committed.
  final (int, int, int)? dragPreview;

  @override
  void paint(Canvas canvas, Size size) {
    final viewport = Offset.zero & size;
    canvas.clipRect(viewport);
    canvas.drawRect(viewport, Paint()..color = ShortedColors.surface);
    final content = Rect.fromLTRB(
      layout.headerWidth,
      0,
      size.width,
      size.height,
    );
    final fromUs = layout.timeForX(content.left);
    final toUs = layout.timeForX(content.right);

    canvas.save();
    canvas.clipRect(content);
    _paintRuler(canvas, size, fromUs, toUs);
    for (var index = 0; index < tracks.length; index++) {
      _paintTrack(canvas, size, tracks[index], index, fromUs, toUs);
    }
    _paintDrag(canvas);
    canvas.restore();

    _paintHeaders(canvas);
  }

  void _paintRuler(Canvas canvas, Size size, int fromUs, int toUs) {
    final step = layout.rulerStep();
    final stepUs = (step * 1e6).round();
    final tick = Paint()..color = ShortedColors.line;
    for (var t = fromUs - fromUs % stepUs; t <= toUs; t += stepUs) {
      if (t < 0) continue;
      final x = layout.xForTime(t);
      canvas.drawLine(
        Offset(x, layout.rulerHeight - 6),
        Offset(x, layout.rulerHeight),
        tick,
      );
      _text(
        canvas,
        formatTimecode(t),
        Offset(x + 3, 2),
        ShortedColors.textMuted,
      );
    }
  }

  void _paintTrack(
    Canvas canvas,
    Size size,
    TimelineTrack track,
    int index,
    int fromUs,
    int toUs,
  ) {
    final top = layout.rulerHeight + index * layout.trackHeight;
    canvas.drawLine(
      Offset(layout.headerWidth, top),
      Offset(size.width, top),
      Paint()..color = ShortedColors.line,
    );
    final fill = Paint()..color = _trackColor(track.kind);
    for (final item in TimelineLayout.visible(track.items, fromUs, toUs)) {
      final rect = Rect.fromLTRB(
        layout.xForTime(item.startUs),
        top + _itemGap,
        layout.xForTime(item.endUs),
        top + layout.trackHeight - _itemGap,
      );
      final shape = RRect.fromRectAndRadius(rect, _itemRadius);
      canvas.drawRRect(shape, fill);
      if (item.id == selectedId) {
        canvas.drawRRect(
          shape,
          Paint()
            ..color = ShortedColors.text
            ..style = PaintingStyle.stroke
            ..strokeWidth = 2,
        );
      }
    }
  }

  void _paintDrag(Canvas canvas) {
    final drag = dragPreview;
    if (drag == null) return;
    final (index, startUs, endUs) = drag;
    final top = layout.rulerHeight + index * layout.trackHeight;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(
          layout.xForTime(startUs),
          top + _itemGap,
          layout.xForTime(endUs),
          top + layout.trackHeight - _itemGap,
        ),
        _itemRadius,
      ),
      Paint()
        ..color = ShortedColors.accent
        ..style = PaintingStyle.stroke
        ..strokeWidth = 3,
    );
  }

  void _paintHeaders(Canvas canvas) {
    final height = layout.rulerHeight + tracks.length * layout.trackHeight;
    canvas.drawRect(
      Rect.fromLTWH(0, 0, layout.headerWidth, height),
      Paint()..color = ShortedColors.surfaceRaised,
    );
    for (var index = 0; index < tracks.length; index++) {
      final top = layout.rulerHeight + index * layout.trackHeight;
      _text(
        canvas,
        tracks[index].label,
        Offset(8, top + 10),
        ShortedColors.text,
      );
    }
  }

  void _text(Canvas canvas, String value, Offset at, Color color) {
    final painter = TextPainter(
      text: TextSpan(
        text: value,
        style: TextStyle(color: color, fontSize: 11, fontFamily: 'Inter'),
      ),
      maxLines: 1,
      textDirection: textDirection,
    )..layout();
    painter.paint(canvas, at);
    painter.dispose();
  }

  @override
  bool shouldRepaint(TimelinePainter old) =>
      old.layout != layout ||
      old.tracks != tracks ||
      old.durationUs != durationUs ||
      old.selectedId != selectedId ||
      old.dragPreview != dragPreview;
}

/// Driven by `playheadUs` directly, so no widget rebuilds during a scrub.
class PlayheadPainter extends CustomPainter {
  PlayheadPainter({required this.layout, required this.playheadUs})
    : super(repaint: playheadUs);

  final TimelineLayout layout;
  final ValueNotifier<int> playheadUs;

  @override
  void paint(Canvas canvas, Size size) {
    final x = layout.xForTime(playheadUs.value);
    if (x < layout.headerWidth) return;
    canvas.drawLine(
      Offset(x, 0),
      Offset(x, size.height),
      Paint()
        ..color = ShortedColors.accent
        ..strokeWidth = 2,
    );
  }

  @override
  bool shouldRepaint(PlayheadPainter old) =>
      old.layout != layout || old.playheadUs != playheadUs;
}
