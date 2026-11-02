import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/arrange.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import '../timeline/timeline_layout.dart';

/// The local analysis reasons are codes; an AI writes its own words, shown as they are.
String reasonText(AppLocalizations l10n, String? reason) => switch (reason) {
  null => '',
  'silence' => l10n.reasonSilence,
  'blur' => l10n.reasonBlur,
  'shake' => l10n.reasonShake,
  'dark' => l10n.reasonDark,
  'bright' => l10n.reasonBright,
  final other => other,
};

String _seconds(int us) => (us / 1e6).toStringAsFixed(1);

String decisionText(AppLocalizations l10n, DecisionView decision) {
  final clip = decision.clipNumber;
  final removed = _seconds(decision.removedUs.toInt());
  final why = reasonText(l10n, decision.reason);
  final what = switch (decision.kind) {
    DecisionKindView.trimmed => l10n.decisionTrimmed(clip, removed),
    DecisionKindView.cut => l10n.decisionCut(clip, removed),
    DecisionKindView.dropped => l10n.decisionDropped(clip),
    DecisionKindView.split => l10n.decisionSplit(clip),
    DecisionKindView.moved => l10n.decisionMoved(clip),
  };
  return why.isEmpty ? what : '$what · $why';
}

IconData _icon(DecisionKindView kind) => switch (kind) {
  DecisionKindView.trimmed => Icons.content_cut,
  DecisionKindView.cut => Icons.cut,
  DecisionKindView.dropped => Icons.delete_outline,
  DecisionKindView.split => Icons.vertical_split_outlined,
  DecisionKindView.moved => Icons.swap_vert,
};

/// What the review closes with.
enum ArrangeOutcome { keep, adjust, undone }

/// S12: before and after length, every decision, doubts in amber with "Check", and Keep ·
/// Adjust · Undo all.
class ArrangeReview extends ConsumerWidget {
  const ArrangeReview({
    super.key,
    required this.projectId,
    required this.review,
  });

  final String projectId;
  final ArrangeReviewView review;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final text = Theme.of(context).textTheme;

    Future<void> undoAll() async {
      try {
        await ref
            .read(editorProvider(projectId).notifier)
            .apply((gateway) => gateway.undoArrangement());
        if (context.mounted) Navigator.pop(context, ArrangeOutcome.undone);
      } on Object catch (error) {
        if (context.mounted) showToast(context, errorMessage(l10n, error));
      }
    }

    void check(String clipId) {
      ref.read(selectedClipProvider.notifier).set(clipId);
      Navigator.pop(context, ArrangeOutcome.adjust);
    }

    return Scaffold(
      appBar: AppBar(title: Text(l10n.arrangeReviewTitle)),
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.all(ShortedSpace.l),
              child: Text(
                l10n.arrangeBeforeAfter(
                  formatTimecode(review.beforeUs.toInt()),
                  formatTimecode(review.afterUs.toInt()),
                ),
                style: text.headlineSmall,
              ),
            ),
            if (review.summary.isNotEmpty)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.l),
                child: Text(review.summary),
              ),
            Expanded(
              child: ListView(
                padding: const EdgeInsets.all(ShortedSpace.m),
                children: [
                  for (final doubt in review.uncertain)
                    Card(
                      color: ShortedColors.attentionSoft,
                      child: ListTile(
                        leading: const Icon(
                          Icons.help_outline,
                          color: ShortedColors.attention,
                        ),
                        title: Text(
                          l10n.decisionUncertain(doubt.clipNumber),
                          style: const TextStyle(
                            color: ShortedColors.attention,
                          ),
                        ),
                        subtitle: Text(doubt.reason),
                        trailing: TextButton(
                          onPressed: () => check(doubt.clipId),
                          child: Text(l10n.check),
                        ),
                      ),
                    ),
                  if (review.decisions.isEmpty)
                    Padding(
                      padding: const EdgeInsets.all(ShortedSpace.m),
                      child: Text(
                        l10n.arrangeNothing,
                        style: const TextStyle(color: ShortedColors.textMuted),
                      ),
                    ),
                  for (final decision in review.decisions)
                    ListTile(
                      leading: Icon(_icon(decision.kind)),
                      title: Text(decisionText(l10n, decision)),
                    ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(ShortedSpace.m),
              child: Row(
                children: [
                  Expanded(
                    child: SecondaryButton(
                      label: l10n.undoAll,
                      onPressed: undoAll,
                    ),
                  ),
                  const SizedBox(width: ShortedSpace.s),
                  Expanded(
                    child: SecondaryButton(
                      label: l10n.adjust,
                      onPressed: () =>
                          Navigator.pop(context, ArrangeOutcome.adjust),
                    ),
                  ),
                  const SizedBox(width: ShortedSpace.s),
                  Expanded(
                    child: PrimaryButton(
                      label: l10n.keep,
                      onPressed: () =>
                          Navigator.pop(context, ArrangeOutcome.keep),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
