import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/transcribe.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

final _word = CaptionHitView(
  clipId: 'c1',
  segment: 's1',
  word: 2,
  text: 'hello',
  left: 0.4,
  top: 0.7,
  width: 0.2,
  height: 0.05,
);

Future<FakeGateway> _editor(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  final gateway = FakeGateway()
    ..projects.add(project)
    ..captionWord = _word;
  await open(
    tester,
    '/editor/p1/captions',
    size: const Size(1280, 900),
    gateway: gateway,
  );
  return gateway;
}

void main() {
  testWidgets('one tap on a caption word changes its emphasis', (tester) async {
    final gateway = await _editor(tester);
    await tester.tapAt(tester.getCenter(find.byType(OverlaySelection)));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(gateway.speechCalls, ['emphasis s1 2']);
  });

  testWidgets('two taps open the word for correction, in place', (
    tester,
  ) async {
    final gateway = await _editor(tester);
    final centre = tester.getCenter(find.byType(OverlaySelection));
    await tester.tapAt(centre);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(centre);
    await tester.pumpAndSettle();
    final editor = find.byKey(const ValueKey('preview-caption-editor'));
    expect(editor, findsOneWidget);
    await tester.enterText(editor, 'world');
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pumpAndSettle();
    expect(editor, findsNothing);
    expect(gateway.speechCalls, ['edit s1 2 world']);
  });
}
