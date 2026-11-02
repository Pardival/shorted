import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import 'project_dialogs.dart';
import 'project_providers.dart';

String _reason(AppLocalizations l10n, VersionReasonView reason) =>
    switch (reason) {
      VersionReasonView.activity => l10n.versionAuto,
      VersionReasonView.beforeAi => l10n.versionBeforeAi,
      VersionReasonView.beforeRestore => l10n.versionBeforeRestore,
    };

/// "Version history" (FR-PRJ-04): pick a version, confirm, and it comes back as one edit that
/// Undo takes back.
Future<void> showVersionHistory(
  BuildContext context,
  WidgetRef ref,
  String projectId,
) async {
  final l10n = AppLocalizations.of(context);
  final versions = await ref.read(projectGatewayProvider).versionHistory();
  if (!context.mounted) return;
  final locale = Localizations.localeOf(context).toLanguageTag();
  final format = DateFormat.yMMMd(locale).add_Hm();
  final picked = await showDialog<VersionView>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.versionHistory),
      content: SizedBox(
        width: 360,
        child: versions.isEmpty
            ? Text(
                l10n.noVersions,
                style: const TextStyle(color: ShortedColors.textMuted),
              )
            : ListView(
                shrinkWrap: true,
                children: [
                  for (final version in versions)
                    ListTile(
                      key: ValueKey('version-${version.name}'),
                      leading: const Icon(Icons.history),
                      title: Text(
                        format.format(
                          DateTime.fromMillisecondsSinceEpoch(
                            version.savedAtMs.toInt(),
                          ),
                        ),
                      ),
                      subtitle: Text(_reason(l10n, version.reason)),
                      onTap: () => Navigator.pop(context, version),
                    ),
                ],
              ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
      ],
    ),
  );
  if (picked == null || !context.mounted) return;
  final sure = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.restoreVersion),
      content: Text(l10n.restoreVersionBody),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(l10n.restoreVersion),
        ),
      ],
    ),
  );
  if (sure != true || !context.mounted) return;
  try {
    await ref
        .read(editorProvider(projectId).notifier)
        .apply((gateway) => gateway.restoreVersion(picked.name));
    if (context.mounted) showToast(context, l10n.versionRestored);
  } on Object catch (error) {
    if (context.mounted) showToast(context, errorMessage(l10n, error));
  }
}
