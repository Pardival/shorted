import 'package:flutter/material.dart';

import '../../bridge/error.dart';
import '../../l10n/generated/app_localizations.dart';

const _maxTitleLength = 120;

String errorMessage(AppLocalizations l10n, Object error) {
  if (error is! EngineFailure) return l10n.errorGeneric;
  return switch (error.code) {
    ErrorCode.storage => l10n.errorStorage,
    ErrorCode.newerProject => l10n.errorNewerProject,
    ErrorCode.damagedProject => l10n.errorDamagedProject,
    _ => l10n.errorGeneric,
  };
}

Future<String?> askTitle(BuildContext context, String current) {
  return showDialog<String>(
    context: context,
    builder: (context) => _TitleDialog(current),
  );
}

/// Owns its controller: the field still needs it during the closing animation.
class _TitleDialog extends StatefulWidget {
  const _TitleDialog(this.current);

  final String current;

  @override
  State<_TitleDialog> createState() => _TitleDialogState();
}

class _TitleDialogState extends State<_TitleDialog> {
  late final _controller = TextEditingController(text: widget.current);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return AlertDialog(
      title: Text(l10n.rename),
      content: TextField(
        controller: _controller,
        autofocus: true,
        maxLength: _maxTitleLength,
        decoration: InputDecoration(labelText: l10n.projectTitleLabel),
        onSubmitted: (value) => Navigator.pop(context, value.trim()),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, _controller.text.trim()),
          child: Text(l10n.rename),
        ),
      ],
    );
  }
}

Future<bool> confirmDelete(BuildContext context, String title) async {
  final l10n = AppLocalizations.of(context);
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.deleteConfirmTitle(title)),
      content: Text(l10n.deleteConfirmBody),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(l10n.delete),
        ),
      ],
    ),
  );
  return confirmed ?? false;
}
