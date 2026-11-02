import 'package:flutter/services.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/media.dart';
import 'package:shorted/features/timeline/filmstrips.dart';

ThumbnailView thumbnail(int timeUs) => ThumbnailView(
  timeUs: PlatformInt64Util.from(timeUs),
  width: 1,
  height: 1,
  rgba: Uint8List.fromList([1, 2, 3, 255]),
);

void main() {
  testWidgets('a strip fills in and finds the frame shown at a time', (
    tester,
  ) async {
    var loads = 0;
    final strips = Filmstrips((_) {
      loads++;
      return Stream.fromIterable([thumbnail(0), thumbnail(1000000)]);
    });
    addTearDown(strips.dispose);
    var repaints = 0;
    strips.addListener(() => repaints++);

    expect(strips.at('clip', 0), isNull);
    await tester.runAsync(() async {
      strips.request('clip');
      strips.request('clip');
      for (var tries = 0; repaints < 2 && tries < 200; tries++) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
    });

    expect(loads, 1);
    expect(strips.at('clip', 0)!.timeUs, 0);
    expect(strips.at('clip', 999999)!.timeUs, 0);
    expect(strips.at('clip', 1000000)!.timeUs, 1000000);
    expect(strips.at('clip', 9000000)!.timeUs, 1000000);
    expect(strips.at('other', 0), isNull);
  });

  testWidgets('a failing strip leaves the clip without images', (tester) async {
    final strips = Filmstrips(
      (_) => Stream.error(PlatformException(code: 'x')),
    );
    addTearDown(strips.dispose);
    await tester.runAsync(() async {
      strips.request('clip');
      await Future<void>.delayed(Duration.zero);
    });
    expect(strips.at('clip', 0), isNull);
  });
}
