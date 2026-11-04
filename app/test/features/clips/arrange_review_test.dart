import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/arrange.dart';
import 'package:shorted/bridge/runtime.dart';
import 'package:shorted/features/clips/arrange_review.dart';
import 'package:shorted/l10n/generated/app_localizations.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

Future<FakeGateway> _arranged(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.addAll(['c1', 'c2']);
  final gateway = FakeGateway()
    ..projects.add(project)
    ..decisions = [
      DecisionView(
        clipId: 'c1',
        clipNumber: 1,
        kind: DecisionKindView.trimmed,
        reason: 'silence',
        removedUs: big(3200000),
      ),
      DecisionView(
        clipId: 'c2',
        clipNumber: 2,
        kind: DecisionKindView.dropped,
        reason: 'duplicate take of clip 3',
        removedUs: big(4000000),
      ),
    ]
    ..uncertain = [
      const UncertainView(
        clipId: 'c2',
        clipNumber: 2,
        reason: 'two possible takes',
      ),
    ];
  await open(
    tester,
    '/editor/p1/clips',
    size: const Size(1280, 800),
    gateway: gateway,
  );
  await tester.tap(find.text('Smart arrange'));
  await tester.pump();
  gateway.arranging!
    ..add(TaskEvent.started(taskId: BigInt.one))
    ..add(const TaskEvent.progress(fraction: 0.5))
    ..add(const TaskEvent.finished());
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('the review shows lengths, decisions and doubts, then keeps', (
    tester,
  ) async {
    final gateway = await _arranged(tester);
    expect(gateway.arrangeCalls, ['analyse', 'apply']);
    expect(find.byType(ArrangeReview), findsOneWidget);
    expect(find.text('2:46 of footage → 1:04'), findsOneWidget);
    expect(find.text('Clip 1: 3.2 s trimmed · silence'), findsOneWidget);
    expect(
      find.text('Clip 2 removed · duplicate take of clip 3'),
      findsOneWidget,
    );
    expect(find.text('Clip 2: worth a look'), findsOneWidget);

    await tester.tap(find.text('Keep'));
    await tester.pumpAndSettle();
    expect(find.byType(ArrangeReview), findsNothing);
    expect(gateway.arrangeCalls, ['analyse', 'apply']);
  });

  testWidgets('undo all puts the project back', (tester) async {
    final gateway = await _arranged(tester);
    await tester.tap(find.text('Undo all'));
    await tester.pumpAndSettle();
    expect(gateway.arrangeCalls.last, 'undo');
    expect(find.byType(ArrangeReview), findsNothing);
  });

  testWidgets('check closes on the doubtful clip, to adjust it', (
    tester,
  ) async {
    await _arranged(tester);
    await tester.tap(find.text('Check'));
    await tester.pumpAndSettle();
    expect(find.byType(ArrangeReview), findsNothing);
    expect(find.textContaining('drag its edges'), findsOneWidget);
  });

  testWidgets('a retake is worded as said again later', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Builder(
          builder: (context) =>
              Text(reasonText(AppLocalizations.of(context), 'duplicate')),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('said again later'), findsOneWidget);
  });
}
