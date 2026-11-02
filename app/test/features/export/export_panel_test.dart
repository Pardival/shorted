import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/bridge/runtime.dart';
import 'package:shorted/design_system/theme.dart';
import 'package:shorted/features/export/export_panel.dart';
import 'package:shorted/features/project/project_providers.dart';
import 'package:shorted/l10n/generated/app_localizations.dart';

import '../project/fake_gateway.dart';

Future<FakeGateway> pump(WidgetTester tester, int durationUs) async {
  final gateway = FakeGateway();
  await tester.pumpWidget(
    ProviderScope(
      overrides: [projectGatewayProvider.overrideWithValue(gateway)],
      child: MaterialApp(
        theme: shortedTheme(),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: ExportPanel(
            durationUs: durationUs,
            chooseFile: () async => '/out/short.mp4',
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('an empty project cannot export', (tester) async {
    await pump(tester, 0);
    expect(find.text('Add clips to the timeline first.'), findsOneWidget);
    final button = tester.widget<FilledButton>(find.byType(FilledButton));
    expect(button.onPressed, isNull);
  });

  testWidgets('over 2:00 export is blocked with a clear message', (
    tester,
  ) async {
    await pump(tester, 121000000);
    expect(find.text('This short is over the 2:00 limit.'), findsOneWidget);
  });

  testWidgets('settings reach the engine, progress shows, cancel works', (
    tester,
  ) async {
    final gateway = await pump(tester, 4000000);
    await tester.tap(find.text('720p'));
    await tester.tap(find.text('60'));
    await tester.pump();
    await tester.tap(find.text('Export video'));
    await tester.pump();
    expect(gateway.exports.single, '/out/short.mp4 720 60');

    gateway.exportEvents.add(TaskEvent.started(taskId: BigInt.from(9)));
    gateway.exportEvents.add(const TaskEvent.progress(fraction: 0.5));
    await tester.pump();
    await tester.pump();
    expect(find.text('Exporting 50 %'), findsOneWidget);

    await tester.tap(find.text('Cancel export'));
    expect(gateway.cancelled.single, BigInt.from(9));
    gateway.exportEvents.add(const TaskEvent.cancelled());
    await tester.pump();
    await tester.pump();
    expect(find.text('Export video'), findsOneWidget);
  });

  testWidgets('a failure gives a human message', (tester) async {
    final gateway = await pump(tester, 4000000);
    await tester.tap(find.text('Export video'));
    await tester.pump();
    gateway.exportEvents.add(
      const TaskEvent.failed(code: ErrorCode.exportFailed),
    );
    await tester.pump();
    await tester.pump();
    expect(find.textContaining('could not be written'), findsOneWidget);
  });
}
