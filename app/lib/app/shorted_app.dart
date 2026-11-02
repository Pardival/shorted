import 'package:flutter/material.dart';

import '../l10n/generated/app_localizations.dart';

class ShortedApp extends StatelessWidget {
  const ShortedApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      onGenerateTitle: (context) => AppLocalizations.of(context).appTitle,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: ThemeData.dark(useMaterial3: true),
      home: const PlaceholderScreen(),
    );
  }
}

/// Temporary screen until the real Home lands (T-1002).
class PlaceholderScreen extends StatelessWidget {
  const PlaceholderScreen({super.key});

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
          ],
        ),
      ),
    );
  }
}
