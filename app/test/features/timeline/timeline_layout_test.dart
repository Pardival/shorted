import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/features/timeline/timeline_layout.dart';

void main() {
  test('overlapping elements each get a row, and a free row is reused', () {
    final placed = inLanes(const [
      TimelineItem(id: 'a', startUs: 0, endUs: 100),
      TimelineItem(id: 'b', startUs: 50, endUs: 150),
      TimelineItem(id: 'c', startUs: 120, endUs: 200),
    ]);
    expect([for (final item in placed) item.lane], [0, 1, 0]);
    final track = TimelineTrack(
      kind: TrackKind.widgets,
      label: 'W',
      items: placed,
    );
    expect(track.lanes, 2);
    const layout = TimelineLayout(
      pixelsPerSecond: 100,
      scrollUs: 0,
      headerWidth: 0,
      rulerHeight: 0,
      trackHeight: 40,
    );
    expect(layout.itemAt(track, 60, lane: 1)?.id, 'b');
    expect(layout.itemAt(track, 60, lane: 0)?.id, 'a');
    expect(layout.laneAt(30, 0, 2), 1);
  });
}
