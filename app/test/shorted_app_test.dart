import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/app/shorted_app.dart';

void main() {
  testWidgets('placeholder screen shows the app name', (tester) async {
    await tester.pumpWidget(const ShortedApp());
    await tester.pumpAndSettle();
    expect(find.text('Shorted'), findsOneWidget);
    expect(find.byType(Scaffold), findsOneWidget);
  });
}
