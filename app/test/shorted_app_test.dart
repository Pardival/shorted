import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/app/shorted_app.dart';
import 'package:shorted/features/editor/editor_parts.dart';

void resize(WidgetTester tester, Size size) {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
}

Future<void> open(
  WidgetTester tester,
  String location, {
  Size size = const Size(390, 844),
}) async {
  resize(tester, size);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ShortedApp(engineVersion: '1.2.3', initialLocation: location),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('starts on Home and opens a new short on Clips', (tester) async {
    await open(tester, '/');
    expect(find.text('New short'), findsOneWidget);
    await tester.tap(find.text('New short'));
    await tester.pumpAndSettle();
    expect(find.text('Clips tools arrive with their tickets.'), findsOneWidget);
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
