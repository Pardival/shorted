import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../design_system/theme.dart';
import '../l10n/generated/app_localizations.dart';
import 'router.dart';

class ShortedApp extends StatefulWidget {
  const ShortedApp({
    super.key,
    required this.engineVersion,
    this.initialLocation = '/',
    this.measurementScreen,
  });

  final String engineVersion;
  final String initialLocation;

  /// Replaces the whole app in measurement builds.
  final Widget? measurementScreen;

  @override
  State<ShortedApp> createState() => _ShortedAppState();
}

class _ShortedAppState extends State<ShortedApp> {
  late final GoRouter _router = createRouter(
    engineVersion: widget.engineVersion,
    initialLocation: widget.initialLocation,
  );

  @override
  void dispose() {
    _router.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final measurement = widget.measurementScreen;
    if (measurement != null) {
      return MaterialApp(
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        theme: shortedTheme(),
        home: measurement,
      );
    }
    return MaterialApp.router(
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: shortedTheme(),
      routerConfig: _router,
      debugShowCheckedModeBanner: false,
    );
  }
}
