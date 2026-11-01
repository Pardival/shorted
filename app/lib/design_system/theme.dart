import 'package:flutter/material.dart';

import 'tokens.g.dart';

const displayFont = 'Space Grotesk';
const uiFont = 'Inter';

ThemeData shortedTheme() {
  const scheme = ColorScheme.dark(
    surface: ShortedColors.surface,
    onSurface: ShortedColors.text,
    primary: ShortedColors.accent,
    onPrimary: ShortedColors.onAccent,
    secondary: ShortedColors.overlay,
    error: ShortedColors.danger,
    outline: ShortedColors.line,
  );
  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: ShortedColors.bg,
    fontFamily: uiFont,
    dividerColor: ShortedColors.line,
    textTheme: const TextTheme(
      displaySmall: TextStyle(
        fontFamily: displayFont,
        fontSize: 30,
        height: 1.1,
        fontWeight: FontWeight.w700,
      ),
      titleMedium: TextStyle(
        fontFamily: displayFont,
        fontSize: 16,
        height: 1.3,
        fontWeight: FontWeight.w700,
      ),
      bodyMedium: TextStyle(fontSize: 15, height: 1.45),
      labelMedium: TextStyle(
        fontSize: 13,
        height: 1.3,
        fontWeight: FontWeight.w500,
      ),
      labelSmall: TextStyle(
        fontSize: 11,
        height: 1.3,
        fontWeight: FontWeight.w500,
      ),
    ),
    sliderTheme: const SliderThemeData(
      activeTrackColor: ShortedColors.accent,
      thumbColor: ShortedColors.accent,
      inactiveTrackColor: ShortedColors.line,
    ),
    bottomSheetTheme: const BottomSheetThemeData(
      backgroundColor: ShortedColors.surfaceRaised,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(
          top: Radius.circular(ShortedRadius.sheet),
        ),
      ),
    ),
    pageTransitionsTheme: const PageTransitionsTheme(
      builders: {
        TargetPlatform.android: FadeUpwardsPageTransitionsBuilder(),
        TargetPlatform.iOS: FadeUpwardsPageTransitionsBuilder(),
        TargetPlatform.macOS: FadeUpwardsPageTransitionsBuilder(),
        TargetPlatform.windows: FadeUpwardsPageTransitionsBuilder(),
        TargetPlatform.linux: FadeUpwardsPageTransitionsBuilder(),
      },
    ),
  );
}
