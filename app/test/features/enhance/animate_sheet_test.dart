import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/enhance/animate_sheet.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';
import 'package:shorted/features/timeline/timeline_view.dart';

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

  testWidgets('a picked keyframe gets its easing and can be deleted', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    project.overlays.add(
      OverlayView(
        id: 'o5',
        kind: OverlayKindView.shape,
        startUs: big(0),
        endUs: big(3000000),
        locked: false,
        hidden: false,
        hasAnimation: true,
        transform: const TransformView(
          x: 540,
          y: 960,
          scale: 1,
          rotation: 0,
          opacity: 1,
          flip: false,
        ),
        text: null,
        animIn: null,
        animLoop: null,
        animOut: null,
        keyframes: [
          KeyframeView(timeUs: big(1000000), easing: EasingView.easeInOut),
        ],
      ),
    );
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/enhance',
      size: const Size(1280, 900),
      gateway: gateway,
    );
    // Select the element on the Widgets track, then its diamond at 1 s. The tools that
    // appear push the timeline down, so it is measured again.
    await tester.tapAt(
      tester.getTopLeft(find.byType(TimelineView)) + const Offset(200, 40),
    );
    await tester.pump(const Duration(milliseconds: 400));
    await tester.tapAt(
      tester.getTopLeft(find.byType(TimelineView)) + const Offset(128, 40),
    );
    await tester.pump(const Duration(milliseconds: 400));

    await tester.tap(find.text('Spring'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls, ['ease o5 1000000 spring']);
    await tester.tap(find.byTooltip('Delete keyframe'));
    await tester.pumpAndSettle();
    expect(gateway.overlayCalls.last, 'unkey o5 1000000');
  });
}
