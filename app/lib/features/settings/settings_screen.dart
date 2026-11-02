import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import 'app_settings.dart';

const _bytesPerMegabyte = 1024 * 1024;
const _languages = {
  'en': 'English',
  'fr': 'Français',
  'es': 'Español',
  'pt': 'Português',
};

final _cacheBytesProvider = FutureProvider.autoDispose<int>(
  (ref) => ref.read(projectGatewayProvider).cacheBytes(),
);

final _speechModelProvider = Provider.autoDispose(
  (ref) => ref.read(projectGatewayProvider).speechModel(),
);

class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key, required this.engineVersion});

  final String engineVersion;

  Future<void> _clearCaches(BuildContext context, WidgetRef ref) async {
    final l10n = AppLocalizations.of(context);
    try {
      await ref.read(projectGatewayProvider).clearCaches();
      ref.invalidate(_cacheBytesProvider);
      if (context.mounted) showToast(context, l10n.cachesCleared);
    } on Object catch (error) {
      if (context.mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final locale = ref.watch(localeProvider);
    final cache = ref.watch(_cacheBytesProvider).value;
    final gateway = ref.read(projectGatewayProvider);
    final model = ref.watch(_speechModelProvider);
    return Scaffold(
      appBar: AppBar(
        title: Text(l10n.settingsTitle),
        leading: BackButton(onPressed: () => context.go('/home')),
      ),
      body: ListView(
        padding: const EdgeInsets.all(ShortedSpace.m),
        children: [
          _Heading(l10n.settingsLanguage),
          Wrap(
            spacing: ShortedSpace.s,
            runSpacing: ShortedSpace.s,
            children: [
              ShortedChip(
                label: l10n.languageSystem,
                selected: locale == null,
                onTap: () => ref.read(localeProvider.notifier).set(null),
              ),
              for (final MapEntry(key: code, value: name) in _languages.entries)
                ShortedChip(
                  label: name,
                  selected: locale?.languageCode == code,
                  onTap: () =>
                      ref.read(localeProvider.notifier).set(Locale(code)),
                ),
            ],
          ),
          _Heading(l10n.speechModelTitle),
          if (model.ready) ...[
            ShortedListRow(title: l10n.speechModelReady),
            SecondaryButton(
              label: l10n.deleteSpeechModel,
              onPressed: () async {
                await gateway.deleteSpeechModel();
                ref.invalidate(_speechModelProvider);
              },
            ),
          ] else
            ShortedListRow(title: l10n.speechStaysLocal),
          _Heading(l10n.settingsStorage),
          if (cache != null)
            ShortedListRow(
              title: l10n.cacheSize((cache / _bytesPerMegabyte).ceil()),
            ),
          SecondaryButton(
            label: l10n.clearCaches,
            onPressed: () => _clearCaches(context, ref),
          ),
          _Heading(l10n.settingsShortcuts),
          for (final (keys, action) in [
            ('Space', l10n.shortcutPlay),
            ('J / L', l10n.shortcutBackForward),
            ('K', l10n.shortcutPause),
            ('S', l10n.shortcutSplit),
            ('Del', l10n.shortcutDelete),
            ('⌘/Ctrl Z', l10n.shortcutUndo),
            ('⌘/Ctrl ⇧ Z', l10n.shortcutRedo),
            ('← / →', l10n.shortcutNudge),
            ('+ / −', l10n.shortcutZoom),
          ])
            ShortedListRow(title: '$keys — $action'),
          _Heading(l10n.settingsAbout),
          ShortedListRow(title: l10n.aboutEngine(engineVersion)),
          ShortedListRow(title: l10n.sourceCode),
          SecondaryButton(
            label: l10n.openSourceLicences,
            onPressed: () => showLicensePage(context: context),
          ),
        ],
      ),
    );
  }
}

class _Heading extends StatelessWidget {
  const _Heading(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: ShortedSpace.l, bottom: ShortedSpace.s),
    child: Text(text, style: const TextStyle(fontWeight: FontWeight.w600)),
  );
}
