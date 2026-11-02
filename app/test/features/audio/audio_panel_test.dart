import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/audio.dart';
import 'package:shorted/features/timeline/waveforms.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

AudioTrackView track(String id, List<AudioItemView> items) =>
    AudioTrackView(id: id, name: 'Audio 1', muted: false, items: items);

AudioItemView item(String id) => AudioItemView(
  id: id,
  mediaId: 'm',
  role: AudioRoleView.music,
  volume: 1,
  fadeInUs: big(0),
  fadeOutUs: big(0),
  looped: false,
);

void main() {
  testWidgets('a clip can be muted and its volume reaches the engine', (
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
    expect(find.text('Original sound'), findsOneWidget);

    await tester.tap(find.byTooltip('Mute').last);
    await tester.pumpAndSettle();
    expect(gateway.audioCalls.single, startsWith('clip c1 1.0 true'));

    await tester.drag(find.byType(Slider).first, const Offset(-60, 0));
    await tester.pumpAndSettle();
    expect(gateway.audioCalls.last, startsWith('clip c1'));
    expect(gateway.audioCalls.last, isNot(startsWith('clip c1 1.0')));
  });

  testWidgets('removing a track that holds sound asks first', (tester) async {
    final project = FakeProject('p1', 'Mine')
      ..audioTracks.add(track('t1', [item('a1')]));
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/audio',
      size: const Size(1280, 800),
      gateway: gateway,
    );

    await tester.tap(find.byTooltip('Remove track'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(gateway.audioCalls, isEmpty);

    await tester.tap(find.byTooltip('Remove track'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Delete'));
    await tester.pumpAndSettle();
    expect(gateway.audioCalls, ['remove t1']);
  });

  testWidgets('tracks can be added and items looped', (tester) async {
    final project = FakeProject('p1', 'Mine')
      ..audioTracks.add(track('t1', [item('a1')]));
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/audio',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.tap(find.text('Add audio track'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Loop'));
    await tester.pumpAndSettle();
    expect(gateway.audioCalls, ['add track', 'loop a1 true']);
  });

  testWidgets('recording counts in, plays the video and saves the take', (
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
    await tester.tap(find.text('Record voice-over'));
    await tester.pump();
    expect(find.text('3'), findsOneWidget);
    await tester.pump(const Duration(seconds: 3));
    await tester.pump();
    await tester.pump();
    expect(gateway.audioCalls, ['record']);

    await tester.tap(find.text('Stop recording'));
    await tester.pump();
    await tester.pump();
    expect(gateway.audioCalls.last, startsWith('stop'));
  });

  test('waveform levels keep the loudest value of a span', () async {
    final waves = Waveforms((_) async => Uint8List.fromList([0, 255, 51, 0]));
    addTearDown(waves.dispose);
    expect(waves.level('m', 0, 10000), isNull);
    waves.request('m');
    await Future<void>.delayed(Duration.zero);
    expect(waves.level('m', 0, 10000), 0);
    expect(waves.level('m', 0, 40000), 1);
    expect(waves.level('m', 20000, 30000), 0.2);
    expect(waves.level('m', 90000000, 91000000), 0);
  });
}
