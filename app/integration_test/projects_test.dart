import 'dart:async';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/error.dart';
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

  group('media', () {
    // The smallest valid PNG: 1×1 pixel.
    const png = [
      137,
      80,
      78,
      71,
      13,
      10,
      26,
      10,
      0,
      0,
      0,
      13,
      73,
      72,
      68,
      82,
      0,
      0,
      0,
      1,
      0,
      0,
      0,
      1,
      8,
      6,
      0,
      0,
      0,
      31,
      21,
      196,
      137,
      0,
      0,
      0,
      13,
      73,
      68,
      65,
      84,
      120,
      156,
      99,
      0,
      1,
      0,
      0,
      5,
      0,
      1,
      13,
      10,
      45,
      180,
      0,
      0,
      0,
      0,
      73,
      69,
      78,
      68,
      174,
      66,
      96,
      130,
    ];

    Future<String> write(String name, List<int> bytes) async {
      final file = File('${root.path}/$name');
      await file.writeAsBytes(bytes);
      return file.path;
    }

    test('a bad file fails alone, and placing is one undo step', () async {
      await gateway.create('Media');
      final good = await write('good.png', png);
      final other = await write('other.png', [...png, 0]);
      final corrupt = await write('corrupt.png', [1, 2, 3]);
      final unknown = await write('notes.txt', [1]);
      final failed = await gateway.importFiles([good, corrupt, unknown, other]);
      expect(failed, 2);

      var view = await gateway.view();
      expect(view.media, hasLength(2));
      view = await gateway.placeClips([view.media[1].id, view.media[0].id]);
      final video = view.tracks[1].items;
      expect(video, hasLength(2));
      expect(video[1].startUs.toInt(), 3000000);
      expect(view.durationUs.toInt(), 6000000);

      view = await gateway.undo();
      expect(view.tracks[1].items, isEmpty);
      expect(view.media, hasLength(2));
    });

    test('clip edits work and undo one step each', () async {
      await gateway.create('Edits');
      final path = await write('a.png', png);
      await gateway.importFiles([path]);
      var view = await gateway.view();
      view = await gateway.placeClips([view.media.single.id]);
      final clip = view.tracks[1].items.single.id;

      view = await gateway.splitClip(clip, 1000000);
      expect(view.tracks[1].items, hasLength(2));
      expect(view.tracks[1].items[1].startUs.toInt(), 1000000);

      view = await gateway.trimClip(clip, start: false, deltaUs: -500000);
      expect(view.tracks[1].items[0].endUs.toInt(), 500000);

      view = await gateway.duplicateClip(clip);
      expect(view.tracks[1].items, hasLength(3));
      view = await gateway.moveClip(clip, 2);
      expect(view.tracks[1].items[2].id, clip);
      view = await gateway.deleteClip(clip);
      expect(view.tracks[1].items, hasLength(2));

      for (final length in [3, 3, 2, 2, 1]) {
        view = await gateway.undo();
        expect(view.tracks[1].items, hasLength(length));
      }
    });

    test('a deleted file goes offline, relink brings it back', () async {
      final id = await gateway.create('Offline');
      final path = await write('clip.png', png);
      await gateway.importFiles([path]);
      var view = await gateway.view();
      final mediaId = view.media.single.id;
      await gateway.placeClips([mediaId]);
      await gateway.close();
      await File(path).delete();

      await gateway.open(id);
      view = await gateway.view();
      expect(view.media.single.offline, isTrue);

      final moved = await write('moved.png', png);
      view = await gateway.relink(mediaId, moved);
      expect(view.media.single.offline, isFalse);

      final wrong = await write('wrong.png', [...png, 1]);
      await expectLater(
        gateway.relink(mediaId, wrong),
        throwsA(
          isA<EngineFailure>().having(
            (failure) => failure.code,
            'code',
            ErrorCode.differentMedia,
          ),
        ),
      );
    });
  });
}
