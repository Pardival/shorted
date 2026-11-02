import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/app/shorted_app.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/features/editor/editor_parts.dart';
import 'package:shorted/features/project/project_providers.dart';

import 'features/project/fake_gateway.dart';

void resize(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
}

Future<FakeGateway> open(
  WidgetTester tester,
  String location, {
  Size size = const Size(390, 844),
  FakeGateway? gateway,
}) async {
  final fake =
      gateway ?? (FakeGateway()..projects.add(FakeProject('p1', 'Mine')));
  resize(tester, size);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [projectGatewayProvider.overrideWithValue(fake)],
      child: ShortedApp(engineVersion: '1.2.3', initialLocation: location),
    ),
  );
  await tester.pumpAndSettle();
  return fake;
}

Future<void> menu(WidgetTester tester, String action) async {
  await tester.tap(find.byTooltip('Project actions').first);
  await tester.pumpAndSettle();
  await tester.tap(find.text(action).last);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('New short creates a project and opens it on Clips', (
    tester,
  ) async {
    final gateway = await open(tester, '/', gateway: FakeGateway());
    expect(find.text('Your first short starts here'), findsOneWidget);
    await tester.tap(find.text('New short'));
    await tester.pumpAndSettle();
    expect(find.text('Clips tools arrive with their tickets.'), findsOneWidget);
    expect(find.text('Untitled short'), findsOneWidget);
    expect(gateway.projects, hasLength(1));
  });

  testWidgets('Home renames and duplicates', (tester) async {
    final gateway = await open(tester, '/');
    await menu(tester, 'Rename');
    await tester.enterText(find.byType(TextField), 'Coffee');
    await tester.tap(find.text('Rename').last);
    await tester.pumpAndSettle();
    expect(gateway.projects.single.title, 'Coffee');
    await menu(tester, 'Duplicate');
    expect(find.text('Coffee (copy)'), findsOneWidget);
  });

  testWidgets('delete asks first, then restores from the trash', (
    tester,
  ) async {
    final gateway = await open(tester, '/');
    await menu(tester, 'Delete');
    expect(gateway.trashed, isEmpty);
    await tester.tap(find.text('Delete').last);
    await tester.pumpAndSettle();
    expect(gateway.trashed, hasLength(1));
    expect(find.text('Mine'), findsNothing);

    await tester.tap(find.byTooltip('Trash'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(TextButton, 'Restore').last);
    await tester.pumpAndSettle();
    expect(gateway.projects, hasLength(1));
  });

  testWidgets('the editor shows what the engine says', (tester) async {
    final gateway = await open(tester, '/editor/p1/clips');
    expect(find.text('Mine'), findsOneWidget);
    expect(find.text('0:00 / 0:04'), findsOneWidget);

    gateway.saves.add(SaveStateView.saving);
    await tester.pump();
    await tester.pump();
    expect(find.text('Saving…'), findsOneWidget);
    gateway.saves.add(SaveStateView.failed);
    await tester.pump();
    await tester.pump();
    expect(find.text('Not saved'), findsOneWidget);
  });

  testWidgets('rename then undo and redo through the engine', (tester) async {
    await open(tester, '/editor/p1/clips');
    IconButton button(String tooltip) => tester.widget<IconButton>(
      find.widgetWithIcon(
        IconButton,
        tooltip == 'Undo' ? Icons.undo : Icons.redo,
      ),
    );
    expect(button('Undo').onPressed, isNull);

    await tester.tap(find.text('Mine'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), 'New name');
    await tester.tap(find.widgetWithText(TextButton, 'Rename'));
    await tester.pumpAndSettle();
    expect(find.text('New name'), findsOneWidget);

    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(find.text('Mine'), findsOneWidget);
    await tester.tap(find.byTooltip('Redo'));
    await tester.pumpAndSettle();
    expect(find.text('New name'), findsOneWidget);
  });

  testWidgets('a project that cannot open shows a human message', (
    tester,
  ) async {
    final gateway = FakeGateway()
      ..projects.add(FakeProject('p1', 'Mine'))
      ..failOpenWith = ErrorCode.newerProject;
    await open(tester, '/editor/p1/clips', gateway: gateway);
    expect(
      find.text(
        'This project was made with a newer version of Shorted. '
        'Update the app to open it.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('deep link opens the right section', (tester) async {
    await open(tester, '/editor/p1/captions');
    expect(
      find.text('Captions tools arrive with their tickets.'),
      findsOneWidget,
    );
  });

  testWidgets('an unknown section falls back to Clips', (tester) async {
    await open(tester, '/editor/p1/nope');
    expect(find.text('Clips tools arrive with their tickets.'), findsOneWidget);
  });

  testWidgets('layout follows the 600 and 1024 breakpoints', (tester) async {
    await open(tester, '/editor/p1/clips');
    expect(find.byType(SectionBar), findsOneWidget);
    expect(find.byType(SectionRail), findsNothing);

    resize(tester, const Size(800, 900));
    await tester.pumpAndSettle();
    expect(find.byType(SectionRail), findsOneWidget);
    expect(find.byType(Inspector), findsNothing);

    resize(tester, const Size(1280, 800));
    await tester.pumpAndSettle();
    expect(find.byType(Inspector), findsOneWidget);
    expect(find.byType(SectionBar), findsNothing);
  });

  testWidgets('resizing the window keeps the section', (tester) async {
    await open(tester, '/editor/p1/clips', size: const Size(1280, 800));
    await tester.tap(find.text('Audio'));
    await tester.pumpAndSettle();
    resize(tester, const Size(390, 844));
    await tester.pumpAndSettle();
    expect(find.text('Audio tools arrive with their tickets.'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('layouts do not overflow in French on small and large windows', (
    tester,
  ) async {
    tester.platformDispatcher.localesTestValue = const [Locale('fr')];
    addTearDown(tester.platformDispatcher.clearLocalesTestValue);
    for (final size in const [
      Size(320, 640),
      Size(800, 700),
      Size(1280, 720),
    ]) {
      await open(tester, '/editor/p1/enhance', size: size);
      expect(tester.takeException(), isNull, reason: '$size');
      expect(find.text('Enregistré'), findsOneWidget);
    }
  });
}
