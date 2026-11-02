import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/bridge/runtime.dart';
import 'package:shorted/features/clips/arrange_review.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

Future<FakeGateway> _clips(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.addAll(['c1', 'c2']);
  final gateway = FakeGateway()..projects.add(project);
  await open(
    tester,
    '/editor/p1/clips',
    size: const Size(1280, 800),
    gateway: gateway,
  );
  return gateway;
}

Future<void> _menu(WidgetTester tester, String item) async {
  await tester.tap(find.byTooltip('AI'));
  await tester.pumpAndSettle();
  await tester.tap(find.text(item));
  await tester.pumpAndSettle();
}

Future<void> _finish(WidgetTester tester, FakeGateway gateway) async {
  gateway.arranging!
    ..add(TaskEvent.started(taskId: BigInt.two))
    ..add(const TaskEvent.finished());
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('the exact payload is shown, and consent can be remembered', (
    tester,
  ) async {
    final gateway = await _clips(tester);
    await _menu(tester, 'Arrange with AI');
    expect(find.text('Send to the AI?'), findsOneWidget);
    expect(find.text('To anthropic, model claude-sonnet-5-5.'), findsOneWidget);
    final shown = tester.widget<SelectableText>(
      find.byKey(const ValueKey('ai-payload')),
    );
    expect(shown.data, FakeGateway.payload);

    await tester.tap(find.text('Remember for this project'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(gateway.arrangeCalls, ['prepare en', 'send']);
    expect(gateway.consented, {'p1'});
    await _finish(tester, gateway);
    expect(find.byType(ArrangeReview), findsOneWidget);
    await tester.tap(find.text('Keep'));
    await tester.pumpAndSettle();

    // Remembered: the next request goes without the sheet.
    await _menu(tester, 'Arrange with AI');
    expect(find.text('Send to the AI?'), findsNothing);
    expect(gateway.arrangeCalls.last, 'send');
  });

  testWidgets('cancelling sends nothing, and a fallback is told', (
    tester,
  ) async {
    final gateway = await _clips(tester);
    await _menu(tester, 'Arrange with AI');
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(gateway.arrangeCalls, ['prepare en']);

    gateway.fellBack = true;
    await _menu(tester, 'Arrange with AI');
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    await _finish(tester, gateway);
    expect(find.textContaining('the local one is shown'), findsOneWidget);
  });

  testWidgets('without a provider the user is told what to set up', (
    tester,
  ) async {
    final gateway = await _clips(tester)
      ..aiTestFails = ErrorCode.aiNotConfigured;
    await _menu(tester, 'Arrange with AI');
    expect(
      find.text('Choose a provider and add its key first.'),
      findsOneWidget,
    );
    expect(gateway.arrangeCalls, ['prepare en']);
  });

  testWidgets('an instruction goes through its own consent and the review', (
    tester,
  ) async {
    final gateway = await _clips(tester);
    gateway.consented.add('p1');
    await _menu(tester, 'Ask Shorted');
    await tester.enterText(
      find.byKey(const ValueKey('ask-shorted')),
      'put the coffee shot first',
    );
    await tester.tap(find.text('Ask'));
    await tester.pumpAndSettle();
    // Arranging was allowed, but a free instruction is another kind of request.
    expect(find.text('Send to the AI?'), findsOneWidget);
    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(gateway.arrangeCalls, ['ask en put the coffee shot first', 'send']);
    await _finish(tester, gateway);
    expect(find.byType(ArrangeReview), findsOneWidget);
  });
}
