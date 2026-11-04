import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

Future<FakeGateway> _enhance(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  final gateway = FakeGateway()..projects.add(project);
  await open(
    tester,
    '/editor/p1/enhance',
    size: const Size(1280, 900),
    gateway: gateway,
  );
  return gateway;
}

void main() {
  testWidgets('a tap places the item in the middle and selects it', (
    tester,
  ) async {
    final gateway = await _enhance(tester);
    await tester.tap(find.byKey(const ValueKey('library-badges-check')));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls, ['library badges-check 0 centre']);
    expect(find.byTooltip('Bring forward'), findsOneWidget);
  });

  testWidgets('categories and search narrow the items', (tester) async {
    await _enhance(tester);
    expect(find.byKey(const ValueKey('library-arrows-right')), findsOneWidget);

    await tester.tap(find.text('Badges'));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('library-arrows-right')), findsNothing);
    expect(find.byKey(const ValueKey('library-badges-check')), findsOneWidget);

    await tester.tap(find.text('All'));
    await tester.enterText(find.byType(TextField), 'LEFT');
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('library-arrows-left')), findsOneWidget);
    expect(find.byKey(const ValueKey('library-arrows-right')), findsNothing);

    await tester.enterText(find.byType(TextField), 'zebra');
    await tester.pumpAndSettle();
    expect(find.text('Nothing found.'), findsOneWidget);
  });

  testWidgets('dropping an item on the preview places it at the drop point', (
    tester,
  ) async {
    final gateway = await _enhance(tester);
    final area = find.byType(OverlaySelection);
    final scale = tester.getSize(area).width / canvasWidth;
    final target = tester.getTopLeft(area) + Offset(270 * scale, 480 * scale);

    final from = tester.getCenter(
      find.byKey(const ValueKey('library-arrows-right')),
    );
    // Tests run as a phone: an item is lifted with a long press.
    final gesture = await tester.startGesture(from);
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
    await gesture.moveBy(const Offset(-20, 0));
    await tester.pump();
    await gesture.moveTo(target);
    await tester.pump();
    await gesture.up();
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls, ['library arrows-right 0 270 480']);
  });
}
