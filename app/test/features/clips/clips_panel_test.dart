import 'dart:typed_data';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/media.dart';
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

  testWidgets('each file shows its first frame', (tester) async {
    final project = FakeProject('p1', 'Mine');
    final gateway = FakeGateway()
      ..projects.add(project)
      ..thumbnails = [
        ThumbnailView(timeUs: big(0), width: 2, height: 2, rgba: Uint8List(16)),
      ];
    await gateway.open('p1');
    await gateway.importFiles(const ['/a.mov']);
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
    // Pixels are decoded by the engine of the test, outside fake time.
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 200)),
    );
    await tester.pumpAndSettle();
    expect(find.byType(RawImage), findsOneWidget);
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

  testWidgets('a media is removed from the project at once', (tester) async {
    final gateway = await pump(tester, const ['/a.mov', '/b.mov']);
    final id = gateway.opened!.media.first.id;
    await tester.tap(find.byKey(ValueKey('remove-media-$id')));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    expect(gateway.removedMedia, [id]);
    expect(find.byKey(ValueKey('remove-media-$id')), findsNothing);
    expect(gateway.opened!.media, hasLength(1));
  });

  testWidgets('a clip can be renamed; Cancel keeps its name', (tester) async {
    final gateway = await pump(tester, const ['/a.mov']);
    final id = gateway.opened!.media.single.id;
    await tester.tap(find.byKey(ValueKey('rename-media-$id')));
    await tester.pumpAndSettle();
    await tester.enterText(find.byKey(const ValueKey('media-name')), 'Nope');
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.text('Nope'), findsNothing);

    await tester.tap(find.byKey(ValueKey('rename-media-$id')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('media-name')),
      'Plage au matin',
    );
    await tester.tap(find.text('Rename'));
    await tester.pumpAndSettle();
    expect(find.text('Plage au matin'), findsOneWidget);
  });

  testWidgets('press and hold a clip, then drag it down to move it', (
    tester,
  ) async {
    final gateway = await pump(tester, const ['/a.mov', '/b.mov', '/c.mov']);
    final ids = [for (final m in gateway.opened!.media) m.id];
    final first = find.byKey(ValueKey('media-row-${ids.first}'));
    final gesture = await tester.startGesture(tester.getCenter(first));
    await tester.pump(kLongPressTimeout + const Duration(milliseconds: 100));
    final rowHeight = tester.getSize(first).height;
    for (var step = 0; step < 10; step++) {
      await gesture.moveBy(Offset(0, rowHeight * 2.2 / 10));
      await tester.pump();
    }
    await gesture.up();
    await tester.pumpAndSettle();
    expect(gateway.mediaOrders.single, [ids[1], ids[2], ids[0]]);
    final tops = [
      for (final id in ids)
        tester.getTopLeft(find.byKey(ValueKey('media-row-$id'))).dy,
    ];
    expect(tops[0], greaterThan(tops[2]));
  });
}
