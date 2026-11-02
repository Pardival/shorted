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
  scrollingGeometry();
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

  test('overlapping overlays are all found, the latest on top', () {
    const long = TimelineItem(id: 'long', startUs: 0, endUs: 9000000);
    const short = TimelineItem(id: 'short', startUs: 1000000, endUs: 2000000);
    const late = TimelineItem(id: 'late', startUs: 5000000, endUs: 6000000);
    const track = TimelineTrack(
      kind: TrackKind.widgets,
      label: 'W',
      items: [long, short, late],
    );
    final found = TimelineLayout.visible(
      track.items,
      3000000,
      4000000,
      overlapping: true,
    );
    expect(found.map((item) => item.id), ['long']);
    expect(layout.itemAt(track, 1500000)?.id, 'short');
    expect(layout.itemAt(track, 7000000)?.id, 'long');
  });

  testWidgets('an overlay is moved in time, and a locked one stays', (
    tester,
  ) async {
    final playhead = ValueNotifier<int>(0);
    addTearDown(playhead.dispose);
    final timings = <String>[];
    Widget timeline({required bool locked}) => harness(
      TimelineView(
        tracks: [
          TimelineTrack(
            kind: TrackKind.widgets,
            label: 'W',
            items: [
              TimelineItem(
                id: 'o',
                startUs: 1000000,
                endUs: 2000000,
                locked: locked,
                animated: true,
              ),
            ],
          ),
        ],
        durationUs: 4000000,
        playheadUs: playhead,
        selectedId: 'o',
        onOverlayTiming: (item, start, end) =>
            timings.add('${item.id} $start $end'),
      ),
    );
    await tester.pumpWidget(timeline(locked: false));
    final origin = tester.getTopLeft(find.byType(TimelineView));
    // 1 s to 2 s spans x = 128 to 168 at the default 40 px/s.
    await tester.dragFrom(origin + const Offset(148, 40), const Offset(40, 0));
    await tester.pump(const Duration(milliseconds: 400));
    expect(timings, ['o 2000000 3000000']);

    await tester.pumpWidget(timeline(locked: true));
    await tester.dragFrom(origin + const Offset(148, 40), const Offset(40, 0));
    await tester.pump(const Duration(milliseconds: 400));
    expect(timings, hasLength(1));
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

  test('snapping takes the nearest target within the threshold', () {
    expect(snapTime(1000, const [900, 1200], 150), 900);
    expect(snapTime(1000, const [1500], 150), 1000);
    expect(snapTime(1000, const [], 150), 1000);
  });

  testWidgets('dragging the selected item trims or moves it', (tester) async {
    final playhead = ValueNotifier<int>(0);
    addTearDown(playhead.dispose);
    final trims = <String>[];
    final moves = <String>[];
    await tester.pumpWidget(
      harness(
        TimelineView(
          tracks: [
            TimelineTrack(kind: TrackKind.video, label: 'V', items: items(4)),
          ],
          durationUs: 4000000,
          playheadUs: playhead,
          onTrim: (item, start, delta) => trims.add('${item.id} $start $delta'),
          onMove: (item, to) => moves.add('${item.id} $to'),
        ),
      ),
    );
    final origin = tester.getTopLeft(find.byType(TimelineView));
    // Item 1 spans x = 128 to 168 at the default 40 px/s.
    await tester.tapAt(origin + const Offset(148, 40));
    await tester.pump(const Duration(milliseconds: 400));

    await tester.dragFrom(origin + const Offset(168, 40), const Offset(20, 0));
    await tester.pump(const Duration(milliseconds: 400));
    expect(trims, hasLength(1));
    expect(trims.single, startsWith('1 false'));

    await tester.dragFrom(origin + const Offset(148, 40), const Offset(100, 0));
    await tester.pump(const Duration(milliseconds: 400));
    expect(moves, ['1 3']);
    expect(playhead.value, 0);
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

void scrollingGeometry() {
  const layout = TimelineLayout(
    pixelsPerSecond: 40,
    scrollUs: 0,
    headerWidth: 88,
    rulerHeight: 20,
    trackHeight: 30,
    scrollY: 40,
    pinnedIndex: 1,
  );
  test('tracks scroll under a pinned Video track', () {
    expect(layout.topOfTrack(1), 20);
    expect(layout.topOfTrack(3), 70);
    expect(layout.trackAt(25, 5), 1);
    expect(layout.trackAt(75, 5), 3);
    expect(layout.maxScrollY(5, 100), 70);
    expect(layout.maxScrollY(2, 100), 0);
  });
  test('collapsed tracks are the video and thin strips', () {
    final collapsed = layout.copyWith(collapsed: true);
    expect(collapsed.heightOfTrack(1), 30);
    expect(collapsed.heightOfTrack(0), 6);
    expect(collapsed.topOfTrack(1), 20);
    expect(collapsed.topOfTrack(0), 50);
    expect(collapsed.topOfTrack(2), 56);
    expect(collapsed.maxScrollY(5, 40), 0);
    expect(collapsed.trackAt(52, 5), 0);
  });
}
