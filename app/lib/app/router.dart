import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../design_system/motion.dart';
import '../design_system/tokens.g.dart';
import '../features/editor/editor_section.dart';
import '../features/editor/editor_shell.dart';
import '../features/home/home_screen.dart';
import '../features/home/onboarding_screen.dart';
import '../features/home/splash_screen.dart';
import '../features/settings/settings_screen.dart';

GoRouter createRouter({
  required String engineVersion,
  String initialLocation = '/',
}) {
  return GoRouter(
    initialLocation: initialLocation,
    routes: [
      GoRoute(path: '/', builder: (context, state) => const SplashScreen()),
      GoRoute(
        path: '/onboarding',
        builder: (context, state) => const OnboardingScreen(),
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
          return CustomTransitionPage(
            key: ValueKey('editor-$projectId'),
            transitionDuration: ShortedMotion.screen,
            reverseTransitionDuration: ShortedMotion.screen,
            transitionsBuilder: (context, animation, secondary, child) =>
                screenTransition(child, animation),
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
