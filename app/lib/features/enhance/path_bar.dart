import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

/// S40 Path: record a path at half or full speed, then smooth, orient, fit or clear it.
class PathBar extends ConsumerStatefulWidget {
  const PathBar({super.key, required this.overlay, required this.run});

  final OverlayView overlay;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  ConsumerState<PathBar> createState() => _PathBarState();
}

class _PathBarState extends ConsumerState<PathBar> {
  double? _smoothing;

  /// Arms a recording: the playhead goes to the element's start, the finger starts it.
  Future<void> _arm(double rate) async {
    final overlay = widget.overlay;
    if (overlay.path != null) {
      // The element must follow the finger, not the old path, while recording.
      await widget.run((gateway) => gateway.clearPath(overlay.id));
    }
    ref.read(previewTransportProvider).seek(overlay.startUs.toInt());
    ref.read(recordingPathProvider.notifier).set(rate);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final overlay = widget.overlay;
    final path = overlay.path;
    final armed = ref.watch(recordingPathProvider);
    void done() {
      ref.read(recordingPathProvider.notifier).set(null);
      ref.read(pathEditingProvider.notifier).set(null);
    }

    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(
        children: [
          for (final (rate, label) in [
            (0.5, l10n.recordHalfSpeed),
            (1.0, l10n.recordFullSpeed),
          ])
            Padding(
              padding: const EdgeInsets.only(right: ShortedSpace.xs),
              child: ShortedChip(
                label: label,
                selected: armed == rate,
                onTap: () => _arm(rate),
              ),
            ),
          if (armed != null)
            Text(
              l10n.dragToRecord,
              style: const TextStyle(color: ShortedColors.accent),
            ),
          if (path != null && armed == null) ...[
            SizedBox(
              width: 140,
              child: Slider(
                value: _smoothing ?? path.smoothing,
                label: l10n.smoothing,
                semanticFormatterCallback: (_) => l10n.smoothing,
                onChanged: (value) => setState(() => _smoothing = value),
                onChangeEnd: (value) => widget.run(
                  (gateway) => gateway.setPathOptions(
                    overlay.id,
                    smoothing: value,
                    orientToPath: path.orientToPath,
                  ),
                ),
              ),
            ),
            ShortedIconButton(
              icon: path.orientToPath ? Icons.explore : Icons.explore_off,
              tooltip: l10n.orientToPath,
              onPressed: () => widget.run(
                (gateway) => gateway.setPathOptions(
                  overlay.id,
                  smoothing: _smoothing ?? path.smoothing,
                  orientToPath: !path.orientToPath,
                ),
              ),
            ),
            ShortedIconButton(
              icon: Icons.straighten,
              tooltip: l10n.fitPath,
              onPressed: () => widget.run(
                (gateway) => gateway.stretchPath(
                  overlay.id,
                  (overlay.endUs - overlay.startUs).toInt(),
                ),
              ),
            ),
            ShortedIconButton(
              icon: Icons.delete_sweep_outlined,
              tooltip: l10n.clearPath,
              onPressed: () =>
                  widget.run((gateway) => gateway.clearPath(overlay.id)),
            ),
          ],
          TextButton(onPressed: done, child: Text(l10n.doneManaging)),
        ],
      ),
    );
  }
}
