import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/design_system/task_progress.dart';

void main() {
  test('the display follows how long work lasts', () {
    expect(
      taskDisplayFor(const Duration(milliseconds: 299)),
      TaskDisplay.nothing,
    );
    expect(
      taskDisplayFor(const Duration(milliseconds: 300)),
      TaskDisplay.inline,
    );
    expect(
      taskDisplayFor(const Duration(milliseconds: 1999)),
      TaskDisplay.inline,
    );
    expect(taskDisplayFor(const Duration(seconds: 2)), TaskDisplay.overlay);
  });

  testWidgets('short work shows nothing, long work offers cancel', (
    tester,
  ) async {
    var cancelled = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TaskProgress(
            label: 'Working',
            fraction: 0.5,
            cancelLabel: 'Cancel',
            onCancel: () => cancelled++,
          ),
        ),
      ),
    );
    expect(find.text('Working'), findsNothing);
    await tester.pump(taskInlineAfter);
    expect(find.text('Working'), findsOneWidget);
    await tester.pump(taskOverlayAfter - taskInlineAfter);
    await tester.pump();
    expect(find.text('Working'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    expect(cancelled, 1);
  });
}
