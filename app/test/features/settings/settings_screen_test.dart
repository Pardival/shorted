import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/features/settings/app_settings.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

void main() {
  testWidgets('settings show caches, clear them, and change the language', (
    tester,
  ) async {
    final gateway = FakeGateway();
    await open(
      tester,
      '/settings',
      size: const Size(800, 1400),
      gateway: gateway,
    );
    await tester.pumpAndSettle();
    expect(find.text('Caches use 3 MB'), findsOneWidget);
    expect(find.textContaining('Space'), findsOneWidget);

    await tester.tap(find.text('Clear caches'));
    await tester.pumpAndSettle();
    expect(gateway.cleared, 1);

    await tester.tap(find.text('Français'));
    await tester.pumpAndSettle();
    expect(find.text('Vider les caches'), findsOneWidget);
  });

  test('a damaged settings file counts as none', () async {
    final folder = Directory.systemTemp.createTempSync('shorted-settings');
    addTearDown(() => folder.deleteSync(recursive: true));
    final file = File('${folder.path}/settings.json');
    expect(await loadLocale(file), isNull);
    file.writeAsStringSync('{not json');
    expect(await loadLocale(file), isNull);
    file.writeAsStringSync('{"locale": "fr"}');
    expect(await loadLocale(file), const Locale('fr'));
  });
}
