import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/design_system/tokens.g.dart';

double contrast(Color a, Color b) {
  final (light, dark) = a.computeLuminance() > b.computeLuminance()
      ? (a, b)
      : (b, a);
  return (light.computeLuminance() + 0.05) / (dark.computeLuminance() + 0.05);
}

void main() {
  const pairs = {
    'text on bg': (ShortedColors.text, ShortedColors.bg),
    'text on surface': (ShortedColors.text, ShortedColors.surface),
    'text on raised': (ShortedColors.text, ShortedColors.surfaceRaised),
    'muted on bg': (ShortedColors.textMuted, ShortedColors.bg),
    'muted on surface': (ShortedColors.textMuted, ShortedColors.surface),
    'on accent': (ShortedColors.onAccent, ShortedColors.accent),
    'danger text on bg': (ShortedColors.dangerText, ShortedColors.bg),
    'text on media': (ShortedColors.text, ShortedColors.mediaPlaceholder),
  };

  pairs.forEach((name, pair) {
    test('$name meets 4.5:1', () {
      expect(contrast(pair.$1, pair.$2), greaterThanOrEqualTo(4.5));
    });
  });
}
