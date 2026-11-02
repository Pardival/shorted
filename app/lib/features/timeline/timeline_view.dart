import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../../design_system/tokens.g.dart';
import 'timeline_layout.dart';
import 'timeline_painter.dart';

const _headerWidth = 88.0;
const _rulerHeight = 24.0;
const _defaultPixelsPerSecond = 40.0;
const _wheelZoomSpeed = 0.002;

/// Multi-track timeline. Collapsed and expanded modes come with T-0207.
class TimelineView extends StatefulWidget {
  @visibleForTesting
  static const tracksKey = ValueKey('timeline-tracks');

  const TimelineView({
    super.key,
    required this.tracks,
    required this.durationUs,
    required this.playheadUs,
    this.onSelect,
  });

  final List<TimelineTrack> tracks;
  final int durationUs;

  /// Written while scrubbing; playback writes it too.
  final ValueNotifier<int> playheadUs;
  final ValueChanged<TimelineItem?>? onSelect;

  @override
  State<TimelineView> createState() => _TimelineViewState();
}

class _TimelineViewState extends State<TimelineView> {
  TimelineLayout _layout = const TimelineLayout(
    pixelsPerSecond: _defaultPixelsPerSecond,
    scrollUs: 0,
    headerWidth: _headerWidth,
    rulerHeight: _rulerHeight,
    trackHeight: ShortedSize.trackHeight,
  );
  TimelineLayout? _pinchStart;
  String? _selectedId;

  void _scrub(double x) {
    widget.playheadUs.value = _layout
        .timeForX(x)
        .clamp(0, widget.durationUs)
        .toInt();
  }

  void _tap(TapUpDetails details) {
    final position = details.localPosition;
    final index = _layout.trackAt(position.dy, widget.tracks.length);
    if (index == null) {
      _scrub(position.dx);
      return;
    }
    final item = _layout.itemAt(
      widget.tracks[index],
      _layout.timeForX(position.dx),
    );
    setState(() => _selectedId = item?.id);
    widget.onSelect?.call(item);
  }

  void _onPointerSignal(PointerSignalEvent event) {
    if (event is! PointerScrollEvent) return;
    final factor = 1 - event.scrollDelta.dy * _wheelZoomSpeed;
    setState(() => _layout = _layout.zoomed(factor, event.localPosition.dx));
  }

  @override
  Widget build(BuildContext context) {
    return Listener(
      onPointerSignal: _onPointerSignal,
      child: GestureDetector(
        onTapUp: _tap,
        onScaleStart: (_) => _pinchStart = _layout,
        onScaleUpdate: (details) {
          final start = _pinchStart;
          if (details.pointerCount > 1 && start != null) {
            setState(() {
              _layout = start.zoomed(details.scale, details.localFocalPoint.dx);
            });
          } else if (details.pointerCount == 1) {
            _scrub(details.localFocalPoint.dx);
          }
        },
        onScaleEnd: (_) => _pinchStart = null,
        child: Stack(
          fit: StackFit.expand,
          children: [
            RepaintBoundary(
              child: CustomPaint(
                key: TimelineView.tracksKey,
                painter: TimelinePainter(
                  layout: _layout,
                  tracks: widget.tracks,
                  durationUs: widget.durationUs,
                  selectedId: _selectedId,
                  textDirection: Directionality.of(context),
                ),
              ),
            ),
            RepaintBoundary(
              child: CustomPaint(
                painter: PlayheadPainter(
                  layout: _layout,
                  playheadUs: widget.playheadUs,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
