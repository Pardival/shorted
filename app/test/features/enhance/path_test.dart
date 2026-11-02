import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';
import 'package:shorted/features/project/project_providers.dart';

import '../../shorted_app_test.dart' show open;
import '../preview/fake_transport.dart';
import '../project/fake_gateway.dart';

OverlayView _element({PathView? path}) => OverlayView(
  colorable: true,
  color: null,
  path: path,
  keyframes: const [],
  animIn: null,
  animLoop: null,
  animOut: null,
  text: null,
  id: 'o9',
  kind: OverlayKindView.shape,
  startUs: big(0),
  endUs: big(3000000),
  locked: false,
  hidden: false,
  hasAnimation: path != null,
  transform: const TransformView(
    x: 540,
    y: 960,
    scale: 1,
    rotation: 0,
    opacity: 1,
    flip: false,
  ),
);

Future<FakeGateway> _pathMode(WidgetTester tester, {PathView? path}) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  project.overlays.add(_element(path: path));
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
    size: const Size(1280, 900),
    gateway: gateway,
  );
  await tester.tapAt(_at(tester, 540, 960));
  await tester.pumpAndSettle();
  await tester.tap(find.byTooltip('Path'));
  await tester.pumpAndSettle();
  return gateway;
}

Offset _at(WidgetTester tester, double x, double y) {
  final area = find.byType(OverlaySelection);
  final scale = tester.getSize(area).width / canvasWidth;
  return tester.getTopLeft(area) + Offset(x * scale, y * scale);
}

FakeTransport _transport(WidgetTester tester) =>
    ProviderScope.containerOf(tester.element(find.byType(OverlaySelection)))
            .read(previewTransportProvider)
        as FakeTransport;

void main() {
  testWidgets('a path is recorded at half speed while the finger moves', (
    tester,
  ) async {
    final gateway = await _pathMode(tester);
    await tester.tap(find.text('Record at 0.5×'));
    await tester.pumpAndSettle();
    final transport = _transport(tester);
    expect(transport.seeks.last, 0);
    expect(find.text('Drag the element on the preview'), findsOneWidget);

    final gesture = await tester.startGesture(_at(tester, 300, 600));
    for (var step = 1; step <= 6; step++) {
      transport.positionUs = step * 100000;
      await gesture.moveTo(_at(tester, 300 + step * 40.0, 600 + step * 30.0));
      await tester.pump(const Duration(milliseconds: 16));
    }
    await gesture.up();
    await tester.pumpAndSettle();
    expect(transport.rate, 0.5);
    expect(transport.isPlaying, isFalse);
    expect(gateway.previewed, isNotEmpty);
    expect(gateway.overlayCalls.single, startsWith('record o9 '));
    final count = int.parse(gateway.overlayCalls.single.split(' ').last);
    expect(count, greaterThanOrEqualTo(2));
  });

  testWidgets('points move, are deleted with a double tap, and options apply', (
    tester,
  ) async {
    final gateway = await _pathMode(
      tester,
      path: PathView(
        points: [
          PathPointView(timeUs: big(0), x: 200, y: 400),
          PathPointView(timeUs: big(1000000), x: 600, y: 800),
          PathPointView(timeUs: big(2000000), x: 800, y: 1400),
        ],
        smoothing: 0.6,
        orientToPath: false,
      ),
    );
    await tester.dragFrom(_at(tester, 600, 800), const Offset(30, 0));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.single, startsWith('point o9 1 '));

    await tester.tapAt(_at(tester, 800, 1400));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(_at(tester, 800, 1400));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, 'unpoint o9 2');

    await tester.tap(find.byTooltip('Turn along the path'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, 'path o9 0.6 true');
    await tester.tap(find.byTooltip('Stretch over the element'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, 'stretch o9 3000000');
  });
}
