import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/components.dart';
import '../../design_system/motion.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../settings/app_settings.dart';

const _pageCount = 3;
const _illustrationSize = 120.0;
const _illustrationIcon = 56.0;
const _dotSize = 8.0;
const _maxWidth = 480.0;

/// S02: three pages on first launch, skippable, replayable from Settings > About (T-1003).
class OnboardingScreen extends ConsumerStatefulWidget {
  const OnboardingScreen({super.key});

  @override
  ConsumerState<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends ConsumerState<OnboardingScreen> {
  final _pages = PageController();
  var _page = 0;

  @override
  void dispose() {
    _pages.dispose();
    super.dispose();
  }

  /// Home opens at once; the setting is written meanwhile.
  void _finish() {
    unawaited(ref.read(onboardedProvider.notifier).finish());
    context.go('/home');
  }

  void _next() {
    if (_page == _pageCount - 1) {
      _finish();
      return;
    }
    _pages.nextPage(
      duration: motionDuration(context, ShortedMotion.section),
      curve: Curves.easeOutCubic,
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final last = _page == _pageCount - 1;
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: _maxWidth),
            child: Column(
              children: [
                Align(
                  alignment: AlignmentDirectional.centerEnd,
                  child: Padding(
                    padding: const EdgeInsets.all(ShortedSpace.s),
                    child: TextButton(
                      key: const ValueKey('onboarding-skip'),
                      onPressed: _finish,
                      child: Text(l10n.skip),
                    ),
                  ),
                ),
                Expanded(
                  child: PageView(
                    controller: _pages,
                    onPageChanged: (page) => setState(() => _page = page),
                    children: [
                      _Page(
                        icon: Icons.video_library_outlined,
                        title: l10n.onboardingClipsTitle,
                        body: l10n.onboardingClipsBody,
                      ),
                      _Page(
                        icon: Icons.subtitles_outlined,
                        title: l10n.onboardingCaptionsTitle,
                        body: l10n.onboardingCaptionsBody,
                      ),
                      _Page(
                        icon: Icons.auto_awesome_outlined,
                        title: l10n.onboardingMotionTitle,
                        body: l10n.onboardingMotionBody,
                        footer: const _LanguageAndPrivacy(),
                      ),
                    ],
                  ),
                ),
                _Dots(count: _pageCount, current: _page),
                Padding(
                  padding: const EdgeInsets.all(ShortedSpace.l),
                  child: SizedBox(
                    width: double.infinity,
                    child: PrimaryButton(
                      key: const ValueKey('onboarding-next'),
                      label: last ? l10n.onboardingStart : l10n.onboardingNext,
                      onPressed: _next,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _Page extends StatelessWidget {
  const _Page({
    required this.icon,
    required this.title,
    required this.body,
    this.footer,
  });

  final IconData icon;
  final String title;
  final String body;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    return SingleChildScrollView(
      padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.l),
      child: Column(
        spacing: ShortedSpace.m,
        children: [
          const SizedBox(height: ShortedSpace.xl),
          Container(
            width: _illustrationSize,
            height: _illustrationSize,
            decoration: const BoxDecoration(
              color: ShortedColors.surfaceRaised,
              shape: BoxShape.circle,
            ),
            child: Icon(
              icon,
              size: _illustrationIcon,
              color: ShortedColors.accent,
            ),
          ),
          Text(title, style: text.headlineSmall, textAlign: TextAlign.center),
          Text(
            body,
            style: text.bodyLarge?.copyWith(color: ShortedColors.textMuted),
            textAlign: TextAlign.center,
          ),
          ?footer,
        ],
      ),
    );
  }
}

/// Last page: the interface language, chosen from the system by default, and where videos live.
class _LanguageAndPrivacy extends ConsumerWidget {
  const _LanguageAndPrivacy();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final shown = Localizations.localeOf(context).languageCode;
    return Column(
      spacing: ShortedSpace.m,
      children: [
        const SizedBox(height: ShortedSpace.s),
        Text(
          l10n.settingsLanguage,
          style: Theme.of(context).textTheme.labelLarge,
        ),
        Wrap(
          alignment: WrapAlignment.center,
          spacing: ShortedSpace.s,
          runSpacing: ShortedSpace.s,
          children: [
            for (final MapEntry(key: code, value: name)
                in languageNames.entries)
              ShortedChip(
                label: name,
                selected: shown == code,
                onTap: () =>
                    ref.read(localeProvider.notifier).set(Locale(code)),
              ),
          ],
        ),
        Row(
          mainAxisSize: MainAxisSize.min,
          spacing: ShortedSpace.s,
          children: [
            const Icon(
              Icons.lock_outline,
              size: 18,
              color: ShortedColors.textMuted,
            ),
            Flexible(
              child: Text(
                l10n.onboardingPrivacy,
                style: const TextStyle(color: ShortedColors.textMuted),
              ),
            ),
          ],
        ),
      ],
    );
  }
}

class _Dots extends StatelessWidget {
  const _Dots({required this.count, required this.current});

  final int count;
  final int current;

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      spacing: ShortedSpace.s,
      children: [
        for (var index = 0; index < count; index++)
          AnimatedContainer(
            duration: motionDuration(context, ShortedMotion.section),
            width: index == current ? _dotSize * 3 : _dotSize,
            height: _dotSize,
            decoration: BoxDecoration(
              color: index == current
                  ? ShortedColors.accent
                  : ShortedColors.line,
              borderRadius: BorderRadius.circular(_dotSize),
            ),
          ),
      ],
    );
  }
}
