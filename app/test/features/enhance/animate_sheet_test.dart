import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/enhance/animate_sheet.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

void main() {
  testWidgets('presets are picked per slot from moving thumbnails', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/enhance',
      size: const Size(1280, 900),
      gateway: gateway,
    );
    await tester.tap(find.text('Rectangle'));
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Animate'));
    // The thumbnails never stop moving: pump frames instead of settling.
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.byType(AnimateSheet), findsOneWidget);
    expect(find.byType(PresetThumb), findsNWidgets(21));

    final opacities = tester
        .widgetList<Opacity>(
          find.descendant(
            of: find.byKey(const ValueKey('preset-in_-fade')),
            matching: find.byType(Opacity),
          ),
        )
        .map((widget) => widget.opacity)
        .toList();
    expect(opacities.single, inExclusiveRange(0, 1));

    await tester.tap(find.byKey(const ValueKey('preset-in_-pop')));
    await tester.pump();
    await tester.drag(
      find.byKey(const ValueKey('slot-loop')),
      const Offset(-200, 0),
    );
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('preset-loop-bounce')));
    await tester.pump();
    expect(gateway.overlayCalls.skip(1), [
      'preset o0 in_ pop 500000',
      'preset o0 loop bounce 1200000',
    ]);
    expect(presetsFor[AnimSlotView.out], contains(PresetKindView.shrink));
    expect(find.byType(OverlaySelection), findsOneWidget);
  });
}
