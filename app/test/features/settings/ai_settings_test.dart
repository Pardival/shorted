import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/error.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

Future<FakeGateway> _settings(WidgetTester tester) async {
  final gateway = FakeGateway();
  await open(
    tester,
    '/settings',
    size: const Size(800, 1600),
    gateway: gateway,
  );
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('a key is saved, cleared from the field, and tested', (
    tester,
  ) async {
    final gateway = await _settings(tester);
    expect(find.text('AI providers'), findsOneWidget);
    expect(find.text('Test connection'), findsOneWidget);

    await tester.tap(find.text('Google Gemini'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('ai-key')),
      'secret-key-123',
    );
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(gateway.aiCalls, ['save gemini   true']);
    expect(find.text('secret-key-123'), findsNothing);
    expect(find.textContaining('A key is stored'), findsOneWidget);

    await tester.tap(find.text('Test connection'));
    await tester.pumpAndSettle();
    expect(find.text('Connected: the key works.'), findsOneWidget);
  });

  testWidgets('each failure says what to do', (tester) async {
    final gateway = await _settings(tester);
    await tester.enterText(find.byKey(const ValueKey('ai-key')), 'k');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    for (final (code, words) in [
      (ErrorCode.aiKeyRejected, 'refused this key'),
      (ErrorCode.aiRateLimited, 'Too many requests'),
      (ErrorCode.aiUnreachable, 'No answer'),
    ]) {
      gateway.aiTestFails = code;
      await tester.tap(find.text('Test connection'));
      await tester.pumpAndSettle();
      expect(find.textContaining(words), findsOneWidget, reason: '$code');
    }
    await tester.tap(find.text('Remove the stored key'));
    await tester.pumpAndSettle();
    expect(gateway.aiCalls.last, 'forget');
    expect(find.textContaining('A key is stored'), findsNothing);
  });
}
