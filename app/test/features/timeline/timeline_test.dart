import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/design_system/theme.dart';
import 'package:shorted/features/timeline/timeline_layout.dart';
import 'package:shorted/features/timeline/timeline_view.dart';

const layout = TimelineLayout(
  pixelsPerSecond: 100,
  scrollUs: 0,
  headerWidth: 50,
  rulerHeight: 20,
  trackHeight: 30,
);

List<TimelineItem> items(int count) => [
  for (var i = 0; i < count; i++)
    TimelineItem(id: '$i', startUs: i * 1000000, endUs: (i + 1) * 1000000),
];

Widget harness(Widget child) => MaterialApp(
  theme: shortedTheme(),
  home: Scaffold(body: SizedBox(width: 800, height: 200, child: child)),
);

void main() {
  test('time and pixels convert both ways', () {
    expect(layout.xForTime(2000000), 250);
    expect(layout.timeForX(250), 2000000);
  });

  test('zoom keeps the time under the focus', () {
    final focusUs = layout.timeForX(300);
    final zoomed = layout.zoomed(2, 300);
    expect(zoomed.pixelsPerSecond, 200);
    expect((zoomed.timeForX(300) - focusUs).abs(), lessThan(10));
  });

  test('zoom is bounded and scroll never goes negative', () {
    expect(
      layout.zoomed(1e9, 300).pixelsPerSecond,
      TimelineLayout.maxPixelsPerSecond,
    );
    expect(
      layout.zoomed(1e-9, 300).pixelsPerSecond,
      TimelineLayout.minPixelsPerSecond,
    );
    expect(layout.zoomed(0.5, 300).scrollUs, greaterThanOrEqualTo(0));
  });

  test('visible items come from a window, edges excluded', () {
    final found = TimelineLayout.visible(items(100), 2000000, 4000000);
    expect(found.map((item) => item.id), ['2', '3']);
    expect(TimelineLayout.visible(items(3), 9000000, 10000000), isEmpty);
    expect(TimelineLayout.visible(const [], 0, 1), isEmpty);
  });

  test('tracks are found from y', () {
    expect(layout.trackAt(10, 2), isNull);
    expect(layout.trackAt(25, 2), 0);
    expect(layout.trackAt(55, 2), 1);
    expect(layout.trackAt(85, 2), isNull);
  });

  test('ruler marks stay apart when zooming out', () {
    expect(layout.rulerStep(), 1.0);
    expect(layout.zoomed(0.02, 0).rulerStep(), 60.0);
  });

  testWidgets('moving the playhead does not rebuild or repaint the tracks', (
    tester,
  ) async {
    final playhead = ValueNotifier<int>(0);
    addTearDown(playhead.dispose);
    var builds = 0;
    await tester.pumpWidget(
      harness(
        Builder(
          builder: (context) {
            builds++;
            return TimelineView(
              tracks: const [],
              durationUs: 1000000,
              playheadUs: playhead,
            );
          },
        ),
      ),
    );
    final tracks = tester.renderObject<RenderCustomPaint>(
      find.byKey(TimelineView.tracksKey),
    );
    playhead.value = 500000;
    await tester.pump();
    expect(builds, 1);
    expect(tracks.debugNeedsPaint, isFalse);
  });

  test('timecodes', () {
    expect(formatTimecode(0), '0:00');
    expect(formatTimecode(65000000), '1:05');
  });

  testWidgets('dragging scrubs the playhead, clamped to the project', (
    tester,
  ) async {
    final playhead = ValueNotifier<int>(0);
    addTearDown(playhead.dispose);
    await tester.pumpWidget(
      harness(
        TimelineView(
          tracks: [
            TimelineTrack(kind: TrackKind.video, label: 'V', items: items(4)),
          ],
          durationUs: 4000000,
          playheadUs: playhead,
        ),
      ),
    );
    final origin = tester.getTopLeft(find.byType(TimelineView));
    final gesture = await tester.startGesture(origin + const Offset(168, 30));
    await tester.pump();
    await gesture.moveBy(const Offset(40, 0));
    await tester.pump();
    expect(playhead.value, closeTo(3000000, 50000));
    await gesture.moveBy(const Offset(500, 0));
    await tester.pump();
    expect(playhead.value, 4000000);
    await gesture.up();
  });

  testWidgets('tapping an item selects it, tapping empty space clears', (
    tester,
  ) async {
    final playhead = ValueNotifier<int>(0);
    addTearDown(playhead.dispose);
    final selected = <String?>[];
    await tester.pumpWidget(
      harness(
        TimelineView(
          tracks: [
            TimelineTrack(kind: TrackKind.video, label: 'V', items: items(2)),
          ],
          durationUs: 2000000,
          playheadUs: playhead,
          onSelect: (item) => selected.add(item?.id),
        ),
      ),
    );
    final origin = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(origin + const Offset(100, 40));
    await tester.tapAt(origin + const Offset(700, 40));
    await tester.pump(const Duration(milliseconds: 400));
    expect(selected, ['0', null]);
  });
}
