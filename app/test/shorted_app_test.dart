import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/app/shorted_app.dart';

void main() {
  testWidgets('shows the engine version and the streamed counter', (
    tester,
  ) async {
    await tester.pumpWidget(
      ShortedApp(engineVersion: '1.2.3', counter: Stream.value(7)),
    );
    await tester.pumpAndSettle();
    expect(find.text('Shorted'), findsOneWidget);
    expect(find.text('Engine 1.2.3'), findsOneWidget);
    expect(find.text('Counter from the engine: 7'), findsOneWidget);
  });
}
