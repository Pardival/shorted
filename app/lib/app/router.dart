import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../features/editor/editor_section.dart';
import '../features/editor/editor_shell.dart';
import '../features/home/home_screen.dart';
import '../features/settings/settings_screen.dart';
import '../l10n/generated/app_localizations.dart';

GoRouter createRouter({
  required String engineVersion,
  String initialLocation = '/',
}) {
  return GoRouter(
    initialLocation: initialLocation,
    routes: [
      // Splash (T-1002) has nothing to wait for yet: go straight to Home.
      GoRoute(path: '/', redirect: (context, state) => '/home'),
      GoRoute(
        path: '/onboarding',
        builder: (context, state) => const _OnboardingPlaceholder(),
      ),
      GoRoute(path: '/home', builder: (context, state) => const HomeScreen()),
      GoRoute(
        path: '/settings',
        builder: (context, state) =>
            SettingsScreen(engineVersion: engineVersion),
      ),
      GoRoute(
        path: '/editor/:projectId/:section',
        pageBuilder: (context, state) {
          final projectId = state.pathParameters['projectId'] ?? '';
          // One page key per project: changing section must not rebuild the page.
          return NoTransitionPage(
            key: ValueKey('editor-$projectId'),
            child: EditorShell(
              projectId: projectId,
              section: EditorSection.parse(state.pathParameters['section']),
            ),
          );
        },
      ),
    ],
  );
}

/// Replaced by T-1003.
class _OnboardingPlaceholder extends StatelessWidget {
  const _OnboardingPlaceholder();

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Scaffold(
      body: Center(
        child: TextButton(
          onPressed: () => GoRouter.of(context).go('/home'),
          child: Text('${l10n.onboardingTitle} · ${l10n.skip}'),
        ),
      ),
    );
  }
}
