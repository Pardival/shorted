import 'package:flutter/material.dart';

import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_gateway.dart';

/// T-1201: asks for an OpenAI key when none is stored, then for consent to send the sound.
/// True when the user agreed and a key is there.
Future<bool> prepareOpenaiTranscription(
  BuildContext context,
  ProjectGateway gateway,
) async {
  if (!gateway.openaiTranscriptionReady()) {
    final key = await showDialog<String>(
      context: context,
      builder: (context) => const _KeyDialog(),
    );
    if (key == null || key.isEmpty) return false;
    await gateway.saveOpenaiKey(key);
  }
  if (!context.mounted) return false;
  final l10n = AppLocalizations.of(context);
  final agreed = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l10n.openaiConsentTitle),
      content: Text(l10n.openaiConsentBody),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(l10n.aiConsentSend),
        ),
      ],
    ),
  );
  return agreed ?? false;
}

/// Owns its controller: the field still needs it during the closing animation.
class _KeyDialog extends StatefulWidget {
  const _KeyDialog();

  @override
  State<_KeyDialog> createState() => _KeyDialogState();
}

class _KeyDialogState extends State<_KeyDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return AlertDialog(
      title: Text(l10n.openaiKeyTitle),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          TextField(
            controller: _controller,
            autofocus: true,
            obscureText: true,
            autocorrect: false,
            enableSuggestions: false,
            decoration: InputDecoration(labelText: l10n.aiKey),
            onSubmitted: (value) => Navigator.pop(context, value.trim()),
          ),
          const SizedBox(height: ShortedSpace.s),
          Text(
            l10n.aiKeyHelp,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, _controller.text.trim()),
          child: Text(l10n.save),
        ),
      ],
    );
  }
}
