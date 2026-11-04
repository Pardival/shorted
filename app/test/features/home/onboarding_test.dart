import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:shorted/app/shorted_app.dart';
import 'package:shorted/design_system/components.dart';
import 'package:shorted/features/project/project_providers.dart';
import 'package:shorted/features/settings/app_settings.dart';

import '../preview/fake_transport.dart';
import '../project/fake_gateway.dart';

/// A first launch: nothing saved yet.
Future<void> firstLaunch(WidgetTester tester) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(390, 844);
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        projectGatewayProvider.overrideWithValue(FakeGateway()),
        previewTransportProvider.overrideWithValue(FakeTransport()),
        savedOnboardedProvider.overrideWithValue(false),
      ],
      child: const ShortedApp(engineVersion: '1.2.3'),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'the first launch walks three pages, picks a language, and starts',
    (tester) async {
      await firstLaunch(tester);
      expect(find.text('Drop your clips'), findsOneWidget);

      await tester.tap(find.text('Next'));
      await tester.pumpAndSettle();
      expect(find.text('Talk, we caption'), findsOneWidget);
      await tester.tap(find.text('Next'));
      await tester.pumpAndSettle();
      expect(find.text('Make it move'), findsOneWidget);
      expect(find.text('Your videos stay on your device.'), findsOneWidget);

      // The system language is chosen to begin with.
      final chosen = tester
          .widgetList<ShortedChip>(find.byType(ShortedChip))
          .where((chip) => chip.selected);
      expect(chosen.single.label, 'English');
      await tester.tap(find.text('Français'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Commencer'));
      await tester.pumpAndSettle();
      expect(find.text('Nouveau short'), findsOneWidget);
    },
  );

  testWidgets('skip goes home and onboarding does not come back', (
    tester,
  ) async {
    await firstLaunch(tester);
    await tester.tap(find.text('Skip'));
    await tester.pumpAndSettle();
    expect(find.text('New short'), findsOneWidget);

    GoRouter.of(tester.element(find.text('New short'))).go('/');
    await tester.pumpAndSettle();
    expect(find.text('New short'), findsOneWidget);
    expect(find.text('Drop your clips'), findsNothing);
  });

  test('finishing keeps the chosen language in the settings file', () async {
    final folder = Directory.systemTemp.createTempSync('shorted-onboarding');
    addTearDown(() => folder.deleteSync(recursive: true));
    final file = File('${folder.path}/settings.json');
    final container = ProviderContainer(
      overrides: [
        settingsFileProvider.overrideWithValue(file),
        savedOnboardedProvider.overrideWithValue(false),
      ],
    );
    addTearDown(container.dispose);
    final language = container
        .read(localeProvider.notifier)
        .set(const Locale('fr'));
    await container.read(onboardedProvider.notifier).finish();
    await language;
    expect(jsonDecode(file.readAsStringSync()), {
      'locale': 'fr',
      'onboarded': true,
    });
    final saved = await loadSettings(file);
    expect(savedOnboarded(saved), isTrue);
    expect(container.read(onboardedProvider), isTrue);
  });
}
