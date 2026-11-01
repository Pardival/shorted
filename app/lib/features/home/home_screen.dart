import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';

/// Empty state for now: the project list needs storage (T-1002).
class HomeScreen extends StatelessWidget {
  const HomeScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final text = Theme.of(context).textTheme;
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(ShortedSpace.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(l10n.appTitle, style: text.displaySmall),
                  ),
                  ShortedIconButton(
                    icon: Icons.settings_outlined,
                    tooltip: l10n.settingsTitle,
                    onPressed: () => context.go('/settings'),
                  ),
                ],
              ),
              const SizedBox(height: ShortedSpace.xl),
              PrimaryButton(
                label: l10n.newShort,
                icon: Icons.add,
                onPressed: () => context.go('/editor/draft/clips'),
              ),
              const Spacer(),
              Text(
                l10n.emptyHomeTitle,
                textAlign: TextAlign.center,
                style: text.titleMedium,
              ),
              const SizedBox(height: ShortedSpace.s),
              Text(
                l10n.emptyHomeBody,
                textAlign: TextAlign.center,
                style: const TextStyle(color: ShortedColors.textMuted),
              ),
              const Spacer(),
            ],
          ),
        ),
      ),
    );
  }
}
