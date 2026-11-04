import 'package:file_selector_platform_interface/file_selector_platform_interface.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shorted_app_test.dart' show open;
import 'project/fake_gateway.dart';

/// Answers the save dialog and the open dialog.
class _Picker extends FileSelectorPlatform {
  @override
  Future<FileSaveLocation?> getSaveLocation({
    List<XTypeGroup>? acceptedTypeGroups,
    SaveDialogOptions options = const SaveDialogOptions(),
  }) async => FileSaveLocation('/tmp/${options.suggestedName}');

  @override
  Future<XFile?> openFile({
    List<XTypeGroup>? acceptedTypeGroups,
    String? initialDirectory,
    String? confirmButtonText,
  }) async => XFile('/tmp/trip.shorted');
}

void main() {
  testWidgets('a project is exported to a .shorted file and one is imported', (
    tester,
  ) async {
    FileSelectorPlatform.instance = _Picker();
    final gateway = FakeGateway()
      ..projects.add(FakeProject('p1', 'Café: day 1!'));
    await open(tester, '/', gateway: gateway);

    await tester.tap(find.byTooltip('Project actions').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Export as .shorted'));
    await tester.pumpAndSettle();
    expect(gateway.bundles, ['export p1 /tmp/Café day 1.shorted']);
    expect(find.text('Project exported, with all its media.'), findsOneWidget);
    await tester.pumpAndSettle(const Duration(seconds: 5));

    await tester.tap(find.byTooltip('Import a .shorted project'));
    await tester.pumpAndSettle();
    expect(gateway.bundles.last, 'import /tmp/trip.shorted');
    expect(find.text('Imported'), findsOneWidget);
  });
}
