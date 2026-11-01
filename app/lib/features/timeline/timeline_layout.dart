import 'dart:math' as math;

enum TrackKind { widgets, video, captions, voiceOver, audio }

class TimelineItem {
  const TimelineItem({
    required this.id,
    required this.startUs,
    required this.endUs,
  });

  final String id;
  final int startUs;
  final int endUs;
}

class TimelineTrack {
  const TimelineTrack({
    required this.kind,
    required this.label,
    required this.items,
  });

  final TrackKind kind;
  final String label;

  /// Sorted by start, so visible items can be found by bisection.
  final List<TimelineItem> items;
}

/// Time to pixels and back. Pure, so that gestures and painting share one definition.
class TimelineLayout {
  const TimelineLayout({
    required this.pixelsPerSecond,
    required this.scrollUs,
    required this.headerWidth,
    required this.rulerHeight,
    required this.trackHeight,
  });

  static const minPixelsPerSecond = 2.0;
  static const maxPixelsPerSecond = 2000.0;

  final double pixelsPerSecond;
  final int scrollUs;
  final double headerWidth;
  final double rulerHeight;
  final double trackHeight;

  double xForTime(int timeUs) =>
      headerWidth + (timeUs - scrollUs) / 1e6 * pixelsPerSecond;

  int timeForX(double x) =>
      scrollUs + ((x - headerWidth) / pixelsPerSecond * 1e6).round();

  int? trackAt(double y, int trackCount) {
    if (y < rulerHeight) return null;
    final index = (y - rulerHeight) ~/ trackHeight;
    return index < trackCount ? index : null;
  }

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
    );
  }

  /// Items of a sorted list crossing the time window: painting skips everything else.
  static Iterable<TimelineItem> visible(
    List<TimelineItem> items,
    int fromUs,
    int toUs,
  ) sync* {
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

  TimelineItem? itemAt(TimelineTrack track, int timeUs) {
    for (final item in visible(track.items, timeUs, timeUs + 1)) {
      return item;
    }
    return null;
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
