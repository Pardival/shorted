import 'dart:async';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/error.dart';
import '../../bridge/runtime.dart';
import '../../design_system/components.dart';
import '../../design_system/task_progress.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';

const _widths = {720: '720p', 1080: '1080p'};
const _frameRates = [30, 60];
const _maxDurationUs = 120000000;

const _bytesPerMegabyte = 1000 * 1000;

class ExportPanel extends ConsumerStatefulWidget {
  const ExportPanel({super.key, required this.durationUs, this.chooseFile});

  final int durationUs;

  /// Replaces the save panel in tests.
  final Future<String?> Function()? chooseFile;

  @override
  ConsumerState<ExportPanel> createState() => _ExportPanelState();
}

class _ExportPanelState extends ConsumerState<ExportPanel> {
  var _width = 1080;
  var _fps = 30;
  double? _progress;
  BigInt? _taskId;
  StreamSubscription<TaskEvent>? _events;

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }

  Future<String?> _choose() async {
    final chosen = widget.chooseFile;
    if (chosen != null) return chosen();
    final file = await getSaveLocation(suggestedName: 'short.mp4');
    return file?.path;
  }

  void _onEvent(TaskEvent event, AppLocalizations l10n) {
    if (!mounted) return;
    switch (event) {
      case TaskEvent_Started(taskId: final id):
        _taskId = id;
      case TaskEvent_Progress(:final fraction):
        setState(() => _progress = fraction);
      case TaskEvent_Finished():
        setState(() => _progress = null);
        showToast(context, l10n.exportDone);
      case TaskEvent_Cancelled():
        setState(() => _progress = null);
      case TaskEvent_Failed(:final code):
        setState(() => _progress = null);
        showToast(context, errorMessage(l10n, EngineFailure(code: code)));
    }
  }

  Future<void> _start() async {
    final l10n = AppLocalizations.of(context);
    final path = await _choose();
    if (path == null || !mounted) return;
    setState(() => _progress = 0);
    final gateway = ref.read(projectGatewayProvider);
    _events = gateway
        .export(path, width: _width, fps: _fps)
        .listen((event) => _onEvent(event, l10n));
  }

  void _cancel() {
    final id = _taskId;
    if (id != null) ref.read(projectGatewayProvider).cancelTask(id);
  }

  Widget _choice<T>({
    required String label,
    required Map<T, String> options,
    required T selected,
    required ValueChanged<T> onSelected,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: Theme.of(context).textTheme.labelMedium),
        const SizedBox(height: ShortedSpace.xs),
        Wrap(
          spacing: ShortedSpace.s,
          children: [
            for (final entry in options.entries)
              ChoiceChip(
                label: Text(entry.value),
                selected: entry.key == selected,
                onSelected: (_) => onSelected(entry.key),
              ),
          ],
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final progress = _progress;
    final blocked = switch (widget.durationUs) {
      0 => l10n.exportEmpty,
      > _maxDurationUs => l10n.exportTooLong,
      _ => null,
    };
    if (progress != null) {
      return TaskProgress(
        label: l10n.exportProgress((progress * 100).round()),
        fraction: progress,
        cancelLabel: l10n.exportCancel,
        onCancel: _cancel,
      );
    }
    return SingleChildScrollView(
      padding: const EdgeInsets.all(ShortedSpace.l),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        spacing: ShortedSpace.m,
        children: [
          Text(
            l10n.exportTitle,
            style: Theme.of(context).textTheme.titleMedium,
          ),
          _choice(
            label: l10n.exportResolution,
            options: _widths,
            selected: _width,
            onSelected: (value) => setState(() => _width = value),
          ),
          _choice(
            label: l10n.exportFrameRate,
            options: {for (final fps in _frameRates) fps: '$fps'},
            selected: _fps,
            onSelected: (value) => setState(() => _fps = value),
          ),
          if (blocked == null)
            Text(
              l10n.estimatedSize(
                ref
                        .read(projectGatewayProvider)
                        .exportSizeEstimate(_width, _fps, widget.durationUs) ~/
                    _bytesPerMegabyte,
              ),
              style: const TextStyle(color: ShortedColors.textMuted),
            ),
          if (blocked != null)
            Text(
              blocked,
              style: const TextStyle(color: ShortedColors.attention),
            ),
          PrimaryButton(
            label: l10n.exportStart,
            onPressed: blocked == null ? _start : null,
          ),
        ],
      ),
    );
  }
}
