import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

/// Answers every file dialog with `picked`.
class _Picker extends FileSelectorPlatform {
  _Picker(this.picked);

  final List<String> picked;

  @override
  Future<List<XFile>> openFiles({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async => [for (final path in picked) XFile(path)];
}

Future<FakeGateway> _myAssets(WidgetTester tester) async {
  final project = FakeProject('p1', 'Mine')..clips.add('c1');
  final gateway = FakeGateway()..projects.add(project);
  await open(
    tester,
    '/editor/p1/enhance',
    size: const Size(1280, 900),
    gateway: gateway,
  );
  await tester.tap(find.widgetWithText(Tab, 'My assets'));
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('added files are kept, placed with a tap, and removed', (
    tester,
  ) async {
    final gateway = await _myAssets(tester);
    expect(find.textContaining('kept for every project'), findsOneWidget);

    FileSelectorPlatform.instance = _Picker(['/tmp/logo.png', '/tmp/spin.gif']);
    await tester.tap(find.text('Add files'));
    await tester.pumpAndSettle();
    expect(gateway.brand.map((asset) => asset.name), ['spin.gif', 'logo.png']);

    final logo = gateway.brand.last.id;
    await tester.tap(find.byKey(ValueKey('brand-$logo')));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls, ['brand $logo 0 centre']);

    await tester.tap(find.text('Manage'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(ValueKey('brand-$logo')));
    await tester.pumpAndSettle();
    expect(gateway.brand.map((asset) => asset.name), ['spin.gif']);
    expect(gateway.overlayCalls, hasLength(1));
  });

  testWidgets('a file beyond the limits is refused with a message', (
    tester,
  ) async {
    final gateway = await _myAssets(tester);
    FileSelectorPlatform.instance = _Picker(['/tmp/huge.png']);
    await tester.tap(find.text('Add files'));
    await tester.pump();
    expect(find.textContaining('This file is too large'), findsOneWidget);
    expect(gateway.brand, isEmpty);
    await tester.pumpAndSettle(const Duration(seconds: 5));
  });
}
