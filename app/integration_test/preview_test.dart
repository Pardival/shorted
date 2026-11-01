import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shorted/bridge/frb_generated.dart';
import 'package:shorted/features/preview/preview_controller.dart';
import 'package:shorted/features/project/project_gateway.dart';

/// Real file, real decoder, real compositor, real Flutter texture. Needs the clips of
/// `app/assets/spike` (see scripts/make_fixtures.swift); skipped without them.
void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  const clip = 'assets/spike/h264_1080p60.mov';
  final boundary = GlobalKey();

  Future<ui.Image> capture(WidgetTester tester) async {
    return (await tester.runAsync(() async {
      await Future<void>.delayed(const Duration(milliseconds: 300));
      final render =
          boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      return render.toImage();
    }))!;
  }

  Future<ByteData> pixels(WidgetTester tester, ui.Image image) async {
    return (await tester.runAsync(
      () => image.toByteData(format: ui.ImageByteFormat.rawRgba),
    ))!;
  }

  double brightness(ByteData bytes) {
    var total = 0;
    for (var i = 0; i < bytes.lengthInBytes; i += 4) {
      total +=
          bytes.getUint8(i) + bytes.getUint8(i + 1) + bytes.getUint8(i + 2);
    }
    return total / (bytes.lengthInBytes / 4) / 3;
  }

  testWidgets('a placed clip reaches the screen and follows the transport', (
    tester,
  ) async {
    final manifest = await tester.runAsync(
      () => AssetManifest.loadFromAssetBundle(rootBundle),
    );
    if (!manifest!.listAssets().contains(clip)) return;

    await tester.runAsync(RustLib.init);
    final support = (await tester.runAsync(getApplicationSupportDirectory))!;
    final root = Directory('${support.path}/preview-test');
    final path = '${root.path}/clip.mov';
    final gateway = EngineGateway(root.path);
    final transport = EngineTransport();
    await tester.runAsync(() async {
      await root.create(recursive: true);
      final bytes = await rootBundle.load(clip);
      await File(path).writeAsBytes(bytes.buffer.asUint8List());
      await gateway.create('Preview');
      expect(await gateway.importFiles([path]), 0);
      final view = await gateway.view();
      await gateway.placeClips([view.media.single.id]);
    });

    final id = (await tester.runAsync(transport.startTexture))!;
    await tester.pumpWidget(
      Center(
        child: RepaintBoundary(
          key: boundary,
          child: SizedBox(
            width: 180,
            height: 320,
            child: Texture(textureId: id),
          ),
        ),
      ),
    );

    transport.seek(1000000);
    await tester.pump();
    final first = await capture(tester);
    final firstPixels = await pixels(tester, first);

    transport.seek(9000000);
    await tester.pump();
    final second = await capture(tester);
    final secondPixels = await pixels(tester, second);

    expect(brightness(firstPixels), greaterThan(5), reason: 'black frame');
    expect(
      firstPixels.buffer.asUint8List(),
      isNot(equals(secondPixels.buffer.asUint8List())),
      reason: 'the seek did not change the picture',
    );

    transport.seek(0);
    transport.play();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 500)),
    );
    final position = transport.positionUs;
    transport.pause();
    expect(position, inInclusiveRange(300000, 2000000));

    final png = await tester.runAsync(
      () => second.toByteData(format: ui.ImageByteFormat.png),
    );
    await tester.runAsync(() async {
      await File('${support.path}/preview-test.png')
          .writeAsBytes(png!.buffer.asUint8List());
      await transport.stopTexture();
      await gateway.close();
      await root.delete(recursive: true);
    });
  });
}
