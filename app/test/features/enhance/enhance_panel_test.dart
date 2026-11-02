import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
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
    expect(
      find.text(
        'Select a clip on the timeline to change its speed and colour.',
      ),
      findsOneWidget,
    );

    await tester.tap(find.text('Crossfade'));
    await tester.pumpAndSettle();
    expect(gateway.lookCalls, ['transition crossfade']);

    final timeline = tester.getTopLeft(find.byType(TimelineView));
    await tester.tapAt(timeline + const Offset(100, 40));
    await tester.pump(const Duration(milliseconds: 400));

    await tester.tap(find.text('2.0×'));
    await tester.pumpAndSettle();
    expect(gateway.lookCalls.last, 'speed c1 2.0');

    await tester.drag(find.byType(Slider).first, const Offset(60, 0));
    await tester.pumpAndSettle();
    expect(gateway.lookCalls.last, startsWith('color c1'));
  });
}
