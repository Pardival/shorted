import 'dart:math' as math;

enum TrackKind {
  widgets,
  video,

  /// Each clip's own sound, under the video and tied to it.
  originalSound,
  captions,
  voiceOver,
  audio,
}

class TimelineItem {
  const TimelineItem({
    required this.id,
    required this.startUs,
    required this.endUs,
    this.mediaId,
    this.sourceInUs = 0,
    this.speed = 1,
    this.locked = false,
    this.animated = false,
    this.keyframes = const [],
    this.lane = 0,
  });

  final String id;
  final int startUs;
  final int endUs;

  /// Set for clips: what their filmstrip shows.
  final String? mediaId;
  final int sourceInUs;
  final double speed;

  /// Overlays: a locked one cannot be dragged, an animated one shows diamonds.
  final bool locked;
  final bool animated;

  /// Times holding keyframes, on the timeline.
  final List<int> keyframes;

  /// Overlapping elements sit one under the other: the row of the track this one is on.
  final int lane;

  TimelineItem inLane(int lane) => TimelineItem(
    id: id,
    startUs: startUs,
    endUs: endUs,
    mediaId: mediaId,
    sourceInUs: sourceInUs,
    speed: speed,
    locked: locked,
    animated: animated,
    keyframes: keyframes,
    lane: lane,
  );
}

/// Puts each item on the first row free at its start, so none hides another (T-1206).
/// Items must be sorted by start.
List<TimelineItem> inLanes(List<TimelineItem> items) {
  final ends = <int>[];
  return [
    for (final item in items)
      () {
        var lane = ends.indexWhere((end) => end <= item.startUs);
        if (lane < 0) {
          lane = ends.length;
          ends.add(item.endUs);
        } else {
          ends[lane] = item.endUs;
        }
        return item.inLane(lane);
      }(),
  ];
}

class TimelineTrack {
  const TimelineTrack({
    required this.kind,
    required this.label,
    required this.items,
    this.muted = false,
    this.locked = false,
    this.hidden = false,
    this.id,
  });

  final TrackKind kind;
  final String label;

  /// Set for user audio tracks.
  final String? id;
  final bool muted;
  final bool locked;
  final bool hidden;

  /// Sorted by start, so visible items can be found by bisection.
  final List<TimelineItem> items;

  /// Overlays may overlap in time, so their ends are not in order.
  bool get overlapping => kind == TrackKind.widgets;

  /// Rows the track is split into.
  int get lanes => items.fold(0, (most, item) => math.max(most, item.lane + 1));
}

/// Time to pixels and back. Pure, so that gestures and painting share one definition.
class TimelineLayout {
  const TimelineLayout({
    required this.pixelsPerSecond,
    required this.scrollUs,
    required this.headerWidth,
    required this.rulerHeight,
    required this.trackHeight,
    this.scrollY = 0,
    this.pinnedIndex,
    this.collapsed = false,
  });

  static const collapsedPinnedHeight = 30.0;
  static const collapsedStripHeight = 6.0;
  static const minPixelsPerSecond = 2.0;
  static const maxPixelsPerSecond = 2000.0;

  final double pixelsPerSecond;
  final int scrollUs;
  final double headerWidth;
  final double rulerHeight;
  final double trackHeight;

  /// How far the tracks have scrolled up. The ruler and the pinned track stay put.
  final double scrollY;

  /// The Video track, which stays visible however the tracks scroll.
  final int? pinnedIndex;

  /// Phone layout: the pinned track, then a thin strip per other track.
  final bool collapsed;

  double xForTime(int timeUs) =>
      headerWidth + (timeUs - scrollUs) / 1e6 * pixelsPerSecond;

  int timeForX(double x) =>
      scrollUs + ((x - headerWidth) / pixelsPerSecond * 1e6).round();

  double heightOfTrack(int index) {
    if (!collapsed) return trackHeight;
    return index == pinnedIndex ? collapsedPinnedHeight : collapsedStripHeight;
  }

  double topOfTrack(int index) {
    if (collapsed) {
      if (index == pinnedIndex) return rulerHeight;
      final pinned = pinnedIndex;
      final rank = pinned != null && index > pinned ? index - 1 : index;
      return rulerHeight + collapsedPinnedHeight + rank * collapsedStripHeight;
    }
    final natural = rulerHeight + index * trackHeight - scrollY;
    return index == pinnedIndex ? math.max(natural, rulerHeight) : natural;
  }

  /// Total height of the tracks and the ruler, before scrolling.
  double contentHeight(int trackCount) => collapsed
      ? rulerHeight +
            collapsedPinnedHeight +
            (trackCount - 1) * collapsedStripHeight
      : rulerHeight + trackCount * trackHeight;

  double maxScrollY(int trackCount, double viewportHeight) =>
      collapsed ? 0 : math.max(0, contentHeight(trackCount) - viewportHeight);

  /// The row of a split track under `y`.
  int laneAt(double y, int index, int lanes) {
    if (lanes <= 1) return 0;
    final share = heightOfTrack(index) / lanes;
    return ((y - topOfTrack(index)) / share).floor().clamp(0, lanes - 1);
  }

  int? trackAt(double y, int trackCount) {
    if (y < rulerHeight) return null;
    final pinned = pinnedIndex;
    if (pinned != null && pinned < trackCount) {
      final top = topOfTrack(pinned);
      if (y >= top && y < top + heightOfTrack(pinned)) return pinned;
    }
    for (var index = 0; index < trackCount; index++) {
      final top = topOfTrack(index);
      if (y >= top && y < top + heightOfTrack(index)) return index;
    }
    return null;
  }

  TimelineLayout copyWith({
    int? scrollUs,
    double? scrollY,
    int? pinnedIndex,
    bool? collapsed,
  }) => TimelineLayout(
    pixelsPerSecond: pixelsPerSecond,
    scrollUs: scrollUs ?? this.scrollUs,
    headerWidth: headerWidth,
    rulerHeight: rulerHeight,
    trackHeight: trackHeight,
    scrollY: scrollY ?? this.scrollY,
    pinnedIndex: pinnedIndex ?? this.pinnedIndex,
    collapsed: collapsed ?? this.collapsed,
  );

  /// Keeps the time under `focusX` in place, so a pinch zooms around the fingers.
  TimelineLayout zoomed(double factor, double focusX) {
    final focusUs = timeForX(focusX);
    final next = (pixelsPerSecond * factor).clamp(
      minPixelsPerSecond,
      maxPixelsPerSecond,
    );
    final offsetUs = ((focusX - headerWidth) / next * 1e6).round();
    return TimelineLayout(
      pixelsPerSecond: next,
      scrollUs: math.max(0, focusUs - offsetUs),
      headerWidth: headerWidth,
      rulerHeight: rulerHeight,
      trackHeight: trackHeight,
      scrollY: scrollY,
      pinnedIndex: pinnedIndex,
      collapsed: collapsed,
    );
  }

  /// Items of a sorted list crossing the time window: painting skips everything else. Items
  /// that may overlap are scanned from the start, as their ends are not sorted.
  static Iterable<TimelineItem> visible(
    List<TimelineItem> items,
    int fromUs,
    int toUs, {
    bool overlapping = false,
  }) sync* {
    if (overlapping) {
      for (final item in items) {
        if (item.startUs >= toUs) return;
        if (item.endUs > fromUs) yield item;
      }
      return;
    }
    var low = 0;
    var high = items.length;
    while (low < high) {
      final middle = (low + high) ~/ 2;
      if (items[middle].endUs <= fromUs) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    for (var i = low; i < items.length && items[i].startUs < toUs; i++) {
      yield items[i];
    }
  }

  /// The item under `timeUs`, on `lane` when given; among overlapping overlays, the one
  /// that starts last.
  TimelineItem? itemAt(TimelineTrack track, int timeUs, {int? lane}) {
    TimelineItem? found;
    for (final item in visible(
      track.items,
      timeUs,
      timeUs + 1,
      overlapping: track.overlapping,
    )) {
      if (lane != null && item.lane != lane) continue;
      found = item;
      if (!track.overlapping) break;
    }
    return found;
  }

  /// Seconds between ruler marks, so labels stay at least `minGap` pixels apart.
  double rulerStep({double minGap = 64}) {
    for (final step in const [0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0]) {
      if (step * pixelsPerSecond >= minGap) return step;
    }
    return 120;
  }
}

/// The candidate nearest to `timeUs` within `thresholdUs`, or `timeUs` itself.
int snapTime(int timeUs, Iterable<int> candidates, int thresholdUs) {
  var best = timeUs;
  var bestDistance = thresholdUs + 1;
  for (final candidate in candidates) {
    final distance = (candidate - timeUs).abs();
    if (distance < bestDistance) {
      best = candidate;
      bestDistance = distance;
    }
  }
  return best;
}

String formatTimecode(int timeUs) {
  final seconds = timeUs ~/ 1000000;
  final minutes = seconds ~/ 60;
  return '$minutes:${(seconds % 60).toString().padLeft(2, '0')}';
}
