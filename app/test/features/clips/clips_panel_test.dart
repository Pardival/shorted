import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/design_system/theme.dart';
import 'package:shorted/features/clips/clips_panel.dart';
import 'package:shorted/features/project/project_providers.dart';
import 'package:shorted/l10n/generated/app_localizations.dart';

import '../project/fake_gateway.dart';

/// Rebuilds the panel from the editor state, as the real screen does.
class Harness extends ConsumerWidget {
  const Harness({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final state = ref.watch(editorProvider('p1')).value;
    return Scaffold(
      body: state == null
          ? const SizedBox()
          : ClipsPanel(projectId: 'p1', media: state.view.media),
    );
  }
}

Future<FakeGateway> pump(WidgetTester tester, List<String> files) async {
  final project = FakeProject('p1', 'Mine');
  final gateway = FakeGateway()..projects.add(project);
  await gateway.open('p1');
  await gateway.importFiles(files);
  await tester.pumpWidget(
    ProviderScope(
      overrides: [projectGatewayProvider.overrideWithValue(gateway)],
      child: MaterialApp(
        theme: shortedTheme(),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: const Harness(),
      ),
    ),
  );
  await tester.pumpAndSettle();
  return gateway;
}

void main() {
  testWidgets('an empty project invites to add media', (tester) async {
    await pump(tester, const []);
    expect(find.text('Add videos and photos'), findsOneWidget);
    expect(find.text('or drop files here'), findsOneWidget);
    expect(find.textContaining('Place in this order'), findsNothing);
  });

  testWidgets('tap order becomes timeline order', (tester) async {
    final gateway = await pump(tester, const ['a.mov', 'b.mov', 'c.mov']);
    await tester.tap(find.text('c.mov'));
    await tester.tap(find.text('a.mov'));
    await tester.pump();
    expect(find.text('Place in this order (2)'), findsOneWidget);
    await tester.tap(find.text('Place in this order (2)'));
    await tester.pumpAndSettle();
    expect(gateway.projects.single.clips, ['c.mov', 'a.mov']);
    expect(find.textContaining('Place in this order'), findsNothing);
  });

  testWidgets('tapping again takes a clip out of the order', (tester) async {
    await pump(tester, const ['a.mov', 'b.mov']);
    await tester.tap(find.text('a.mov'));
    await tester.tap(find.text('b.mov'));
    await tester.tap(find.text('a.mov'));
    await tester.pump();
    expect(find.text('Place in this order (1)'), findsOneWidget);
  });

  testWidgets('offline media says so and cannot be placed', (tester) async {
    await pump(tester, const ['gone.mov']);
    expect(find.text('Media offline'), findsOneWidget);
    expect(find.text('Relink'), findsOneWidget);
    await tester.tap(find.text('gone.mov'));
    await tester.pump();
    expect(find.textContaining('Place in this order'), findsNothing);
  });
}
