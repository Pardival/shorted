import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/features/captions/captions_panel.dart';
import 'package:shorted/app/shorted_app.dart';
import 'package:shorted/bridge/api/audio.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/features/editor/editor_parts.dart';
import 'package:shorted/features/project/project_providers.dart';
import 'package:shorted/features/timeline/timeline_view.dart';

import 'features/preview/fake_transport.dart';
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
      overrides: [
        projectGatewayProvider.overrideWithValue(fake),
        previewTransportProvider.overrideWithValue(FakeTransport()),
      ],
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
    expect(find.text('Add videos and photos'), findsOneWidget);
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

  testWidgets('play follows the engine and scrubbing moves it', (tester) async {
    final transport = FakeTransport();
    final gateway = FakeGateway()..projects.add(FakeProject('p1', 'Mine'));
    resize(tester, const Size(1280, 800));
    addTearDown(tester.view.reset);
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          projectGatewayProvider.overrideWithValue(gateway),
          previewTransportProvider.overrideWithValue(transport),
        ],
        child: const ShortedApp(
          engineVersion: '1',
          initialLocation: '/editor/p1/clips',
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.byType(Texture), findsOneWidget);

    await tester.tap(find.byTooltip('Play or pause'));
    await tester.pump();
    transport.positionUs = 2000000;
    await tester.pump(const Duration(milliseconds: 16));
    expect(find.text('0:02 / 0:04'), findsOneWidget);
    expect(transport.seeks, isEmpty);

    transport.isPlaying = false;
    await tester.pump(const Duration(milliseconds: 16));
    await tester.pump();
    expect(find.byIcon(Icons.play_arrow), findsOneWidget);

    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(128, 8));
    await tester.pump(const Duration(milliseconds: 400));
    expect(transport.seeks.single, closeTo(1000000, 50000));
  });

  testWidgets('selecting a clip offers tools that reach the engine', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/audio',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    expect(find.byTooltip('Split'), findsNothing);
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    expect(gateway.opened!.clips, ['c1']);
    await tester.tap(find.byTooltip('Split'));
    await tester.pumpAndSettle();
    expect(gateway.clipCalls.single, startsWith('split c1'));
    await tester.tap(find.byTooltip('Delete'));
    await tester.pumpAndSettle();
    expect(gateway.clipCalls.last, 'delete c1');
    expect(find.byTooltip('Split'), findsNothing);
  });

  testWidgets('a clip selected on the timeline opens its own sound', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tap(find.byTooltip('Sound'));
    await tester.pumpAndSettle();
    expect(find.text('Sound of clip 1'), findsOneWidget);
    await tester.tap(find.byTooltip('Mute').last);
    await tester.pumpAndSettle();
    expect(gateway.audioCalls.last, startsWith('clip c1'));
  });

  testWidgets('a crop gesture follows the finger and records one step', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/audio',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byKey(PreviewArea.cropKey), findsNothing);

    await tester.tap(find.byTooltip('Crop'));
    await tester.pump();
    await tester.drag(find.byKey(PreviewArea.cropKey), const Offset(60, 0));
    await tester.pumpAndSettle();
    expect(gateway.cropMoves, isNotEmpty);
    expect(gateway.cropMoves.first.$1, 'c1');
    final moved = gateway.cropMoves.fold(0.0, (sum, move) => sum + move.$2);
    expect(moved, greaterThan(0));
    expect(gateway.cropCommits, ['c1']);

    await tester.tap(find.byTooltip('Reset crop'));
    await tester.pumpAndSettle();
    expect(gateway.cropResets, ['c1']);

    await tester.tap(find.byTooltip('Crop'));
    await tester.pump();
    expect(find.byKey(PreviewArea.cropKey), findsNothing);
  });

  testWidgets('only still images offer the slow zoom', (tester) async {
    final project = FakeProject('p1', 'Mine')
      ..clips.addAll(['video', 'still'])
      ..stills.add('still');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/audio',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    expect(find.byTooltip('Split'), findsOneWidget);
    expect(find.byTooltip('Slow zoom'), findsNothing);

    // The tools take room above the timeline, so it has moved.
    final moved = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(moved + const Offset(148, 40));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tap(find.byTooltip('Slow zoom'));
    await tester.pumpAndSettle();
    expect(gateway.slowZooms, ['still']);
  });

  testWidgets('keyboard shortcuts reach the editor', (tester) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.keyL);
    await tester.pump();
    expect(find.textContaining('0:01'), findsWidgets);

    await tester.sendKeyEvent(LogicalKeyboardKey.keyJ);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    await tester.pumpAndSettle();
    expect(gateway.clipCalls.single, startsWith('split c1'));

    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    expect(gateway.clipCalls.last, 'delete c1');
  });

  testWidgets('a long press on the ruler selects a range for every track', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    final gesture = await tester.startGesture(timeline + const Offset(128, 6));
    await tester.pump(const Duration(milliseconds: 700));
    await gesture.moveBy(const Offset(40, 0));
    await tester.pump();
    await gesture.up();
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Delete range'));
    await tester.pumpAndSettle();
    expect(gateway.rangeCalls, ['delete 1000000 2000000']);
    expect(find.byTooltip('Delete range'), findsNothing);
  });

  testWidgets('a track header opens its menu and reaches the engine', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(20, 40));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Lock'));
    await tester.pumpAndSettle();
    expect(gateway.audioCalls.single, 'flags video null true null');
  });

  testWidgets('on phones the timeline folds and unfolds', (tester) async {
    await open(tester, '/editor/p1/clips');
    await tester.tap(find.text('All tracks'));
    await tester.pumpAndSettle();
    expect(find.text('Collapse timeline'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a sound moves along the timeline and to another track', (
    tester,
  ) async {
    final gateway = FakeGateway();
    final project = FakeProject('p1', 'Mine')
      ..audioTracks.addAll([
        AudioTrackView(
          id: 't1',
          name: 'Audio 1',
          muted: false,
          items: const [],
        ),
        AudioTrackView(
          id: 't2',
          name: 'Audio 2',
          muted: false,
          items: const [],
        ),
      ])
      ..sounds['t1'] = ['s1'];
    gateway.projects.add(project);
    await open(
      tester,
      '/editor/p1/clips',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    final view = find.byType(TimelineView);
    // Below Widgets, Video, Captions and Voice-over.
    Offset at(double x, int track) =>
        tester.getTopLeft(view) + Offset(x, 24 + 36 * track + 12);
    await tester.tapAt(at(108, 1));
    await tester.pump(const Duration(milliseconds: 400));
    final moved = tester.getTopLeft(view);
    Offset after(double x, int track) =>
        moved + Offset(x, 24 + 36 * track + 12);
    final gesture = await tester.startGesture(after(108, 1));
    await tester.pump();
    await gesture.moveTo(after(148, 2));
    await tester.pump();
    await gesture.up();
    await tester.pumpAndSettle();
    expect(gateway.audioCalls.single, 'move s1 1000000 t2');
  });

  testWidgets('the splash hands over to Home', (tester) async {
    await open(tester, '/');
    expect(find.text('New short'), findsOneWidget);
    expect(tester.takeException(), isNull);
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
    expect(find.byType(CaptionsPanel), findsOneWidget);
  });

  testWidgets('an unknown section falls back to Clips', (tester) async {
    await open(tester, '/editor/p1/nope');
    expect(find.text('Add videos and photos'), findsOneWidget);
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
    expect(find.text('Original sound'), findsOneWidget);
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
