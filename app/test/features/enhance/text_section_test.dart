import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

Future<FakeGateway> _textTab(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  final gateway = FakeGateway()..projects.add(project);
  await open(
    tester,
    '/editor/p1/enhance',
    size: const Size(1280, 900),
    gateway: gateway,
  );
  final tab = find.widgetWithText(Tab, 'Text');
  await tester.ensureVisible(tab);
  await tester.pumpAndSettle();
  await tester.tap(tab);
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('a text is added, selected, and its look changed', (
    tester,
  ) async {
    final gateway = await _textTab(tester);
    await tester.tap(find.text('Add text'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls, ['text Your text 0']);

    await tester.tap(find.text('Chip'));
    await tester.pumpAndSettle();
    expect(
      gateway.overlayCalls.last,
      'set o0 Your text Space Grotesk 96 #FFFFFFFF true chip',
    );

    await tester.tap(find.text('Inter'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, contains(' Inter 96 '));

    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, endsWith('false chip'));
  });

  testWidgets('words typed in the panel are recorded when the field is left', (
    tester,
  ) async {
    final gateway = await _textTab(tester);
    await tester.tap(find.text('Add text'));
    await tester.pumpAndSettle();

    final field = find.byKey(const ValueKey('text-words'));
    await tester.enterText(field, 'Line one\nLine two');
    expect(gateway.overlayCalls, hasLength(1));
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, startsWith('set o0 Line one|Line two '));
  });

  testWidgets('a double tap on the preview types into the text there', (
    tester,
  ) async {
    final gateway = await _textTab(tester);
    await tester.tap(find.text('Add text'));
    await tester.pumpAndSettle();
    gateway.boxes = [
      OverlayBoxView(
        id: 'o0',
        centerX: 540,
        centerY: 960,
        width: 500,
        height: 120,
        rotation: 0,
      ),
    ];
    final centre = tester.getCenter(find.byType(OverlaySelection));
    await tester.tapAt(centre);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(centre);
    await tester.pumpAndSettle();

    final editor = find.byKey(const ValueKey('preview-text-editor'));
    expect(editor, findsOneWidget);
    await tester.enterText(editor, 'On the video');
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pumpAndSettle();
    expect(editor, findsNothing);
    expect(gateway.overlayCalls.last, startsWith('set o0 On the video '));
  });
}
