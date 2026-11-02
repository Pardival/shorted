import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/components.dart';
import '../../l10n/generated/app_localizations.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key, required this.engineVersion});

  final String engineVersion;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(l10n.settingsTitle),
        leading: BackButton(onPressed: () => context.go('/home')),
      ),
      body: ListView(
        children: [ShortedListRow(title: l10n.aboutEngine(engineVersion))],
      ),
    );
  }
}
