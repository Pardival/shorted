@Tags(['golden'])
library;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/design_system/components.dart';
import 'package:shorted/design_system/theme.dart';

void main() {
  testWidgets('components', (tester) async {
    await tester.binding.setSurfaceSize(const Size(360, 420));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        theme: shortedTheme(),
        debugShowCheckedModeBanner: false,
        home: Scaffold(
          body: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              spacing: 12,
              children: [
                PrimaryButton(
                  label: 'New short',
                  icon: Icons.add,
                  onPressed: () {},
                ),
                SecondaryButton(label: 'Adjust', onPressed: () {}),
                ShortedIconButton(
                  icon: Icons.undo,
                  tooltip: 'Undo',
                  onPressed: () {},
                ),
                const Row(
                  spacing: 8,
                  children: [
                    ShortedChip(label: 'Pop'),
                    ShortedChip(label: 'Fade', selected: true),
                  ],
                ),
                ShortedListRow(
                  title: 'Storage',
                  subtitle: '1.2 GB',
                  onTap: () {},
                ),
                const ProgressRing(value: 0.6),
              ],
            ),
          ),
        ),
      ),
    );
    await expectLater(
      find.byType(Scaffold),
      matchesGoldenFile('goldens/components.png'),
    );
  });
}
