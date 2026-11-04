import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/projects.dart';

import '../../shorted_app_test.dart' show open;
import 'fake_gateway.dart';

void main() {
  testWidgets('a version is picked, confirmed and restored', (tester) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()
      ..projects.add(project)
      ..versions.addAll([
        VersionView(
          name: '1793000600000-ai.json',
          savedAtMs: big(1793000600000),
          reason: VersionReasonView.beforeAi,
        ),
        VersionView(
          name: '1793000000000-auto.json',
          savedAtMs: big(1793000000000),
          reason: VersionReasonView.activity,
        ),
      ]);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.tap(find.byTooltip('Version history'));
    await tester.pumpAndSettle();
    expect(find.text('Before an AI change'), findsOneWidget);
    expect(find.text('While editing'), findsOneWidget);

    await tester.tap(
      find.byKey(const ValueKey('version-1793000000000-auto.json')),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('Restore this version').last);
    await tester.pumpAndSettle();
    expect(gateway.restored, ['1793000000000-auto.json']);
    expect(find.text('Version restored.'), findsOneWidget);
  });

  testWidgets('an empty history says when versions come', (tester) async {
    final gateway = FakeGateway()..projects.add(FakeProject('p1', 'Mine'));
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.tap(find.byTooltip('Version history'));
    await tester.pumpAndSettle();
    expect(find.textContaining('every ten minutes'), findsOneWidget);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(gateway.restored, isEmpty);
  });
}
