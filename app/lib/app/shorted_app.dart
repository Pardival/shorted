import 'package:flutter/material.dart';

import '../l10n/generated/app_localizations.dart';

class ShortedApp extends StatelessWidget {
  const ShortedApp({
    super.key,
    required this.engineVersion,
    required this.counter,
  });

  final String engineVersion;
  final Stream<int> counter;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: ThemeData.dark(useMaterial3: true),
      home: PlaceholderScreen(engineVersion: engineVersion, counter: counter),
    );
  }
}

/// Temporary screen until the real Home lands (T-1002).
class PlaceholderScreen extends StatelessWidget {
  const PlaceholderScreen({
    super.key,
    required this.engineVersion,
    required this.counter,
  });

  final String engineVersion;
  final Stream<int> counter;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final text = Theme.of(context).textTheme;
    return Scaffold(
      body: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(l10n.appTitle, style: text.displayMedium),
            const SizedBox(height: 8),
            Text(l10n.placeholderTagline, style: text.bodyLarge),
            const SizedBox(height: 24),
            Text(l10n.engineVersion(engineVersion), style: text.bodyMedium),
            StreamBuilder<int>(
              stream: counter,
              builder: (context, snapshot) => Text(
                l10n.engineCounter(snapshot.data ?? 0),
                style: text.bodyMedium,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
