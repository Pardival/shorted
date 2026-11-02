import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/transcribe.dart';
import 'package:shorted/bridge/runtime.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

TranscriptView transcript() => TranscriptView(
  clipId: 'c1',
  language: 'en',
  segments: [
    SegmentView(
      id: 's1',
      hidden: false,
      words: [
        WordView(index: 0, text: 'Hello', startUs: big(0), deleted: false),
        WordView(index: 1, text: 'gone', startUs: big(9), deleted: true),
      ],
    ),
  ],
);

void main() {
  testWidgets('without a model the panel offers it, and shows its size', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/captions',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.tap(find.text('Download the speech model (60 MB)'));
    await tester.pump();
    expect(gateway.speechCalls, ['download']);
    gateway.speechEvents.add(TaskEvent.started(taskId: BigInt.from(4)));
    gateway.speechEvents.add(const TaskEvent.finished());
    await tester.pump();
    await tester.pump();
    gateway.modelReady = true;
  });

  testWidgets('transcript words can be fixed and phrases hidden', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..transcripts.add(transcript());
    final gateway = FakeGateway()
      ..projects.add(project)
      ..modelReady = true;
    await open(
      tester,
      '/editor/p1/captions',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    expect(find.text('Hello'), findsOneWidget);
    expect(find.text('gone'), findsNothing);

    await tester.tap(find.byTooltip('Hide from captions'));
    await tester.pumpAndSettle();
    expect(gateway.speechCalls, ['hide s1 true']);

    await tester.tap(find.text('Hello'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'Hi');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(gateway.speechCalls.last, 'edit s1 0 Hi');
  });
}
