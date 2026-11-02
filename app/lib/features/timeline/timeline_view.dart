import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../design_system/tokens.g.dart';
import 'timeline_layout.dart';
import 'filmstrips.dart';
import 'timeline_painter.dart';
import 'waveforms.dart';

const _headerWidth = 88.0;
const _rulerHeight = 24.0;
const _defaultPixelsPerSecond = 40.0;
const _wheelZoomSpeed = 0.002;
const _handleWidth = 14.0;
const _snapPixels = 8.0;

enum _DragKind { scrub, trimStart, trimEnd, move }

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
    this.onTrim,
    this.onMove,
    this.filmstrips,
    this.waveforms,
    this.rangeUs,
    this.onRange,
    this.zoomRequests,
  });

  final List<TimelineTrack> tracks;
  final int durationUs;

  /// Written while scrubbing; playback writes it too.
  final ValueNotifier<int> playheadUs;
  final ValueChanged<TimelineItem?>? onSelect;

  /// A dragged edge of the selected item: the item, true for its start, the change in time.
  final void Function(TimelineItem item, bool start, int deltaUs)? onTrim;

  /// The selected item dragged to a new position among its track.
  final void Function(TimelineItem item, int toIndex)? onMove;
  final Filmstrips? filmstrips;
  final Waveforms? waveforms;

  /// The range selected on the ruler, owned by the parent.
  final (int, int)? rangeUs;
  final ValueChanged<(int, int)?>? onRange;

  /// Each value multiplies the zoom, around the playhead.
  final Stream<double>? zoomRequests;

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
  _DragKind _drag = _DragKind.scrub;
  TimelineItem? _dragItem;
  int _dragTrack = 0;
  double _dragStartX = 0;
  (int, int, int)? _dragPreview;
  int? _lastSnap;
  var _pointers = 0;
  int? _rangeAnchor;
  StreamSubscription<double>? _zoomRequests;

  @override
  void initState() {
    super.initState();
    _zoomRequests = widget.zoomRequests?.listen(_zoomBy);
  }

  @override
  void dispose() {
    unawaited(_zoomRequests?.cancel());
    super.dispose();
  }

  void _zoomBy(double factor) {
    final focusX = _layout.xForTime(widget.playheadUs.value);
    setState(() => _layout = _layout.zoomed(factor, focusX));
  }

  int _clampedTime(double x) =>
      _layout.timeForX(x).clamp(0, widget.durationUs).toInt();

  void _rangeStart(LongPressStartDetails details) {
    if (details.localPosition.dy >= _rulerHeight) return;
    _rangeAnchor = _clampedTime(details.localPosition.dx);
  }

  void _rangeMove(LongPressMoveUpdateDetails details) {
    final anchor = _rangeAnchor;
    if (anchor == null) return;
    final time = _clampedTime(details.localPosition.dx);
    if (time == anchor) return;
    widget.onRange?.call(time < anchor ? (time, anchor) : (anchor, time));
  }

  void _pointerDown(PointerDownEvent event) {
    _pointers++;
    if (_pointers == 1) {
      _beginDrag(event.localPosition);
    } else {
      // A second finger means zoom: the edit is dropped.
      setState(() => _dragPreview = null);
      _dragItem = null;
      _drag = _DragKind.scrub;
    }
  }

  void _pointerGone(PointerEvent event) {
    _pointers--;
    if (_drag != _DragKind.scrub) _endDrag();
    _drag = _DragKind.scrub;
  }

  /// Decides at touch down what this drag does: only the selected item can be edited.
  void _beginDrag(Offset position) {
    _drag = _DragKind.scrub;
    final index = _layout.trackAt(position.dy, widget.tracks.length);
    if (index == null || _selectedId == null) return;
    final time = _layout.timeForX(position.dx);
    final slop = (_handleWidth / _layout.pixelsPerSecond * 1e6).round();
    for (final item in widget.tracks[index].items) {
      if (item.id != _selectedId) continue;
      _dragItem = item;
      _dragTrack = index;
      _dragStartX = position.dx;
      if ((time - item.startUs).abs() <= slop) {
        _drag = _DragKind.trimStart;
      } else if ((time - item.endUs).abs() <= slop) {
        _drag = _DragKind.trimEnd;
      } else if (time > item.startUs && time < item.endUs) {
        _drag = _DragKind.move;
      }
    }
  }

  int _snapped(int timeUs, TimelineItem item) {
    final threshold = (_snapPixels / _layout.pixelsPerSecond * 1e6).round();
    final targets = <int>[
      widget.playheadUs.value,
      for (final other in widget.tracks[_dragTrack].items)
        if (other.id != item.id) ...[other.startUs, other.endUs],
    ];
    final snapped = snapTime(timeUs, targets, threshold);
    if (snapped != timeUs && snapped != _lastSnap) {
      HapticFeedback.selectionClick();
    }
    _lastSnap = snapped == timeUs ? null : snapped;
    return snapped;
  }

  void _updateDrag(double x) {
    final item = _dragItem;
    if (item == null) return;
    final deltaUs = ((x - _dragStartX) / _layout.pixelsPerSecond * 1e6).round();
    setState(() {
      _dragPreview = switch (_drag) {
        _DragKind.trimStart => (
          _dragTrack,
          _snapped(
            item.startUs + deltaUs,
            item,
          ).clamp(0, item.endUs - 1).toInt(),
          item.endUs,
        ),
        _DragKind.trimEnd => (
          _dragTrack,
          item.startUs,
          _snapped(item.endUs + deltaUs, item),
        ),
        _ => (_dragTrack, item.startUs + deltaUs, item.endUs + deltaUs),
      };
    });
  }

  void _endDrag() {
    final item = _dragItem;
    final preview = _dragPreview;
    setState(() => _dragPreview = null);
    _dragItem = null;
    if (item == null || preview == null) return;
    final (_, start, end) = preview;
    switch (_drag) {
      case _DragKind.trimStart:
        widget.onTrim?.call(item, true, start - item.startUs);
      case _DragKind.trimEnd:
        widget.onTrim?.call(item, false, end - item.endUs);
      case _DragKind.move:
        final items = widget.tracks[_dragTrack].items;
        final centre = (start + end) ~/ 2;
        final to = items.where((other) {
          return other.id != item.id &&
              (other.startUs + other.endUs) ~/ 2 < centre;
        }).length;
        if (to != items.indexOf(item)) widget.onMove?.call(item, to);
      case _DragKind.scrub:
        break;
    }
  }

  void _scrub(double x) {
    widget.playheadUs.value = _layout
        .timeForX(x)
        .clamp(0, widget.durationUs)
        .toInt();
  }

  void _tap(TapUpDetails details) {
    final position = details.localPosition;
    if (widget.rangeUs != null) widget.onRange?.call(null);
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
      // Edits use raw pointers: the scale gesture only starts after 36 pixels, far too much
      // for a precise trim.
      onPointerDown: _pointerDown,
      onPointerMove: (event) {
        if (_drag != _DragKind.scrub) _updateDrag(event.localPosition.dx);
      },
      onPointerUp: _pointerGone,
      onPointerCancel: _pointerGone,
      onPointerSignal: _onPointerSignal,
      child: GestureDetector(
        onTapUp: _tap,
        onLongPressStart: _rangeStart,
        onLongPressMoveUpdate: _rangeMove,
        onLongPressEnd: (_) => _rangeAnchor = null,
        onScaleStart: (_) => _pinchStart = _layout,
        onScaleUpdate: (details) {
          final start = _pinchStart;
          if (details.pointerCount > 1 && start != null) {
            setState(() {
              _layout = start.zoomed(details.scale, details.localFocalPoint.dx);
            });
          } else if (details.pointerCount == 1 && _drag == _DragKind.scrub) {
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
                  dragPreview: _dragPreview,
                  filmstrips: widget.filmstrips,
                  waveforms: widget.waveforms,
                  rangeUs: widget.rangeUs,
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
