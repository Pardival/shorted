import 'package:flutter/material.dart';

import '../../bridge/api/arrange.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';

/// The answer to the consent sheet.
typedef Consent = ({bool send, bool remember});

/// docs/07 §4: the provider and model, and exactly what is sent, before anything leaves the
/// device. "Remember for this project" skips the sheet next time.
Future<Consent> askAiConsent(
  BuildContext context,
  AiPayloadView payload,
) async {
  final answer = await showModalBottomSheet<Consent>(
    context: context,
    isScrollControlled: true,
    backgroundColor: ShortedColors.surface,
    builder: (context) => _ConsentSheet(payload: payload),
  );
  return answer ?? (send: false, remember: false);
}

class _ConsentSheet extends StatefulWidget {
  const _ConsentSheet({required this.payload});

  final AiPayloadView payload;

  @override
  State<_ConsentSheet> createState() => _ConsentSheetState();
}

class _ConsentSheetState extends State<_ConsentSheet> {
  bool _remember = false;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final payload = widget.payload;
    final height = MediaQuery.sizeOf(context).height;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(ShortedSpace.m),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              l10n.aiConsentTitle,
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: ShortedSpace.s),
            Text(l10n.aiConsentTo(payload.provider, payload.model)),
            const SizedBox(height: ShortedSpace.xs),
            Text(
              l10n.aiConsentWhat,
              style: const TextStyle(color: ShortedColors.textMuted),
            ),
            const SizedBox(height: ShortedSpace.s),
            ConstrainedBox(
              constraints: BoxConstraints(maxHeight: height * 0.4),
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: ShortedColors.bg,
                  borderRadius: BorderRadius.circular(ShortedRadius.s),
                ),
                child: SingleChildScrollView(
                  padding: const EdgeInsets.all(ShortedSpace.s),
                  child: SelectableText(
                    payload.body,
                    key: const ValueKey('ai-payload'),
                    style: const TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 11,
                    ),
                  ),
                ),
              ),
            ),
            CheckboxListTile(
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              value: _remember,
              title: Text(l10n.aiConsentRemember),
              onChanged: (value) => setState(() => _remember = value ?? false),
            ),
            Row(
              children: [
                Expanded(
                  child: SecondaryButton(
                    label: l10n.cancel,
                    onPressed: () =>
                        Navigator.pop(context, (send: false, remember: false)),
                  ),
                ),
                const SizedBox(width: ShortedSpace.s),
                Expanded(
                  child: PrimaryButton(
                    label: l10n.aiConsentSend,
                    onPressed: () => Navigator.pop(context, (
                      send: true,
                      remember: _remember,
                    )),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
