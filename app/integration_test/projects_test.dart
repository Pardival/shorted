import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/frb_generated.dart';
import 'package:shorted/features/project/project_gateway.dart';

/// The whole path against the real engine and real files.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  late Directory root;
  late EngineGateway gateway;

  setUpAll(RustLib.init);
  setUp(() async {
    root = await Directory.systemTemp.createTemp('shorted');
    gateway = EngineGateway(root.path);
  });
  tearDown(() async {
    await gateway.close();
    await root.delete(recursive: true);
  });

  test('create, rename, undo, close, reopen', () async {
    final id = await gateway.create('First');
    await gateway.rename(id, 'Second');
    expect((await gateway.view()).title, 'Second');
    expect((await gateway.view()).canUndo, isTrue);
    expect((await gateway.undo()).title, 'First');
    expect((await gateway.redo()).title, 'Second');
    await gateway.close();

    final list = await gateway.list();
    expect(list.single.title, 'Second');
    await gateway.open(id);
    final view = await gateway.view();
    expect(view.title, 'Second');
    expect(view.tracks.map((t) => t.kind), [
      TrackKindView.widgets,
      TrackKindView.video,
      TrackKindView.captions,
      TrackKindView.voiceOver,
    ]);
  });

  test('duplicate, trash and restore', () async {
    final id = await gateway.create('Original');
    await gateway.duplicate(id, 'Copy');
    expect(await gateway.list(), hasLength(2));
    await gateway.moveToTrash(id);
    expect((await gateway.list()).single.title, 'Copy');
    expect((await gateway.trash()).single.title, 'Original');
    await gateway.restore(id);
    expect(await gateway.list(), hasLength(2));
  });

  test('save events report the journal', () async {
    final events = <SaveStateView>[];
    final saved = Completer<void>();
    final subscription = gateway.saveEvents().listen((event) {
      events.add(event);
      if (event == SaveStateView.saved && !saved.isCompleted) {
        saved.complete();
      }
    });
    final id = await gateway.create('Saved');
    await gateway.rename(id, 'Renamed');
    await saved.future;
    // Not awaited: the engine keeps the sink until the next listener replaces it.
    unawaited(subscription.cancel());
    expect(events.first, SaveStateView.saving);
  });
}
