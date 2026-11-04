import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/bridge/api/overlays.dart';
import 'package:shorted/features/editor/editor_parts.dart';
import 'package:shorted/features/enhance/enhance_panel.dart';
import 'package:shorted/features/enhance/overlay_selection.dart';
import 'package:shorted/features/timeline/timeline_view.dart';

import '../../shorted_app_test.dart' show open;
import '../project/fake_gateway.dart';

void main() {
  testWidgets('a selected clip gets speed and colour; transitions go to all', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()..projects.add(project);
    await open(
      tester,
      '/editor/p1/enhance',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    await tester.ensureVisible(find.widgetWithText(Tab, 'Clip'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(Tab, 'Clip'));
    await tester.pumpAndSettle();
    expect(
      find.text(
        'Select a clip on the timeline to change its speed and colour.',
      ),
      findsOneWidget,
    );

    await tester.ensureVisible(find.widgetWithText(Tab, 'Transitions'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(Tab, 'Transitions'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Crossfade'));
    await tester.pumpAndSettle();
    expect(gateway.lookCalls, ['transition crossfade']);

    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.ensureVisible(find.widgetWithText(Tab, 'Clip'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(Tab, 'Clip'));
    await tester.pumpAndSettle();

    // The Properties column shows the same controls: use the section's.
    final panel = find.byType(EnhancePanel);
    await tester.tap(find.descendant(of: panel, matching: find.text('2.0×')));
    await tester.pumpAndSettle();
    expect(gateway.lookCalls.last, 'speed c1 2.0');

    await tester.drag(
      find.descendant(of: panel, matching: find.byType(Slider)).first,
      const Offset(60, 0),
    );
    await tester.pumpAndSettle();
    expect(gateway.lookCalls.last, startsWith('color c1'));
  });

  testWidgets(
    'a shape is added at the playhead, selected, and gets its tools',
    (tester) async {
      final project = FakeProject('p1', 'Mine')..clips.add('c1');
      final gateway = FakeGateway()..projects.add(project);
      await open(
        tester,
        '/editor/p1/enhance',
        size: const Size(1280, 800),
        gateway: gateway,
      );

      await tester.tap(find.text('Arrow'));
      await tester.pumpAndSettle();
      expect(gateway.overlayCalls, ['shape arrow 0']);

      await tester.tap(find.byTooltip('Bring forward'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Lock'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Duplicate'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Delete'));
      await tester.pumpAndSettle();
      expect(gateway.overlayCalls.skip(1), [
        'layer o0 true',
        'flags o0 true false',
        'duplicate o0',
        'delete o0',
      ]);
      expect(find.byTooltip('Bring forward'), findsNothing);
    },
  );

  testWidgets('a tap on the preview selects the element under it', (
    tester,
  ) async {
    final project = FakeProject('p1', 'Mine')..clips.add('c1');
    final gateway = FakeGateway()
      ..projects.add(project)
      ..boxes = [
        OverlayBoxView(
          id: 'o9',
          centerX: 540,
          centerY: 960,
          width: 400,
          height: 200,
          rotation: 0,
        ),
      ];
    project.overlays.add(
      OverlayView(
        colorable: true,
        color: null,
        path: null,
        keyframes: const [],
        animIn: null,
        animLoop: null,
        animOut: null,
        text: null,
        id: 'o9',
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
      ),
    );
    await open(
      tester,
      '/editor/p1/enhance',
      size: const Size(1280, 800),
      gateway: gateway,
    );
    expect(find.byTooltip('Bring forward'), findsNothing);

    await tester.tapAt(tester.getCenter(find.byType(OverlaySelection)));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Bring forward'), findsOneWidget);
    final painter = find.descendant(
      of: find.byType(PreviewArea),
      matching: find.byWidgetPredicate(
        (widget) => widget is CustomPaint && widget.painter is SelectionPainter,
      ),
    );
    final selection =
        tester.widget<CustomPaint>(painter).painter! as SelectionPainter;
    expect(selection.box?.id, 'o9');

    final corner = tester.getTopLeft(find.byType(OverlaySelection));
    await tester.tapAt(corner + const Offset(4, 4));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Bring forward'), findsNothing);
  });

  test('a turned box contains what it covers on screen', () {
    final box = OverlayBoxView(
      id: 'b',
      centerX: 500,
      centerY: 500,
      width: 400,
      height: 100,
      rotation: 90,
    );
    expect(boxContains(box, const Offset(500, 680)), isTrue);
    expect(boxContains(box, const Offset(680, 500)), isFalse);
  });
}
