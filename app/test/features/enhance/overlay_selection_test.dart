import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

const _start = TransformView(
  x: 540,
  y: 960,
  scale: 1,
  rotation: 0,
  opacity: 1,
  flip: false,
);

Future<FakeGateway> _editor(WidgetTester tester, {bool locked = false}) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  project.overlays.add(
    OverlayView(
      text: null,
      id: 'o9',
      kind: OverlayKindView.shape,
      startUs: big(0),
      endUs: big(3000000),
      locked: locked,
      hidden: false,
      hasAnimation: false,
      transform: _start,
    ),
  );
  final gateway = FakeGateway()
    ..projects.add(project)
    ..boxes = [
      OverlayBoxView(
        id: 'o9',
        centerX: 540,
        centerY: 960,
        width: 360,
        height: 360,
        rotation: 0,
      ),
    ];
  await open(
    tester,
    '/editor/p1/enhance',
    size: const Size(1280, 800),
    gateway: gateway,
  );
  return gateway;
}

/// Canvas units to screen pixels on the preview.
Offset _at(WidgetTester tester, double x, double y) {
  final area = find.byType(OverlaySelection);
  final scale = tester.getSize(area).width / canvasWidth;
  return tester.getTopLeft(area) + Offset(x * scale, y * scale);
}

void main() {
  test('a centre near the middle lines snaps onto them', () {
    final near = snapCentre(const Offset(545, 950), const Size(100, 100), 12);
    expect(near.centre, const Offset(540, 960));
    expect(near.guides, {Guide.centreX, Guide.centreY});
    final far = snapCentre(const Offset(600, 700), const Size(100, 100), 12);
    expect(far.centre, const Offset(600, 700));
    expect(far.guides, isEmpty);
  });

  test('an edge near the safe zone snaps onto it', () {
    final top = snapCentre(const Offset(300, 345), const Size(100, 100), 12);
    expect(top.centre.dy, safeTop + 50);
    expect(top.guides, {Guide.safeTop});
    final bottom = snapCentre(
      const Offset(300, 1385),
      const Size(100, 100),
      12,
    );
    expect(bottom.centre.dy, safeBottom - 50);
    expect(bottom.guides, {Guide.safeBottom});
  });

  testWidgets('dragging an element moves it live and records it once', (
    tester,
  ) async {
    final gateway = await _editor(tester);
    final from = _at(tester, 540, 960);
    final gesture = await tester.startGesture(from);
    for (var step = 1; step <= 5; step++) {
      await gesture.moveTo(from + Offset(0, step * 20.0));
      await tester.pump();
    }
    await gesture.up();
    await tester.pumpAndSettle();
    expect(gateway.previewed, isNotEmpty);
    expect(gateway.previewed.last.y, greaterThan(960));
    expect(gateway.overlayCalls, hasLength(1));
    expect(gateway.overlayCalls.single, startsWith('transform o9 540 '));
  });

  testWidgets('a corner handle scales and the top handle turns', (
    tester,
  ) async {
    final gateway = await _editor(tester);
    await tester.tapAt(_at(tester, 540, 960));
    await tester.pumpAndSettle();

    await tester.dragFrom(_at(tester, 720, 1140), const Offset(30, 30));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, contains(' 0 1.0 false'));
    expect(gateway.previewed.last.scale, greaterThan(1));

    final scale = tester.getSize(find.byType(OverlaySelection)).width / 1080;
    final knob = _at(tester, 540, 780) - Offset(0, 24);
    await tester.dragFrom(knob, Offset(180 * scale + 24, 180 * scale + 24));
    await tester.pumpAndSettle();
    expect(gateway.previewed.last.rotation, greaterThan(30));
    expect(gateway.overlayCalls, hasLength(2));
  });

  testWidgets('a locked element does not move', (tester) async {
    final gateway = await _editor(tester, locked: true);
    await tester.dragFrom(_at(tester, 540, 960), const Offset(0, 80));
    await tester.pumpAndSettle();
    expect(gateway.previewed, isEmpty);
    expect(gateway.overlayCalls, isEmpty);
  });

  testWidgets('opacity and flip are set from the panel', (tester) async {
    final gateway = await _editor(tester);
    await tester.tapAt(_at(tester, 540, 960));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Flip'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, endsWith('1.0 true'));
  });
}
