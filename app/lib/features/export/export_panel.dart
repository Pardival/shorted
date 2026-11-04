import 'dart:async';
import 'dart:io';

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
import '../timeline/timeline_layout.dart';

const _widths = {720: '720p', 1080: '1080p'};
const _uhdWidth = 2160;
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

class _ExportPanelState extends ConsumerState<ExportPanel>
    with WidgetsBindingObserver {
  var _width = 1080;
  var _fps = 30;
  var _srt = false;

  /// Where the cover picture is taken, or none (FR-EXP-05).
  int? _coverUs;
  var _paused = false;
  double? _progress;
  String? _savedPath;
  BigInt? _taskId;
  StreamSubscription<TaskEvent>? _events;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    unawaited(_events?.cancel());
    super.dispose();
  }

  /// Phones may not use the GPU in the background: the export waits there and goes on when the
  /// app comes back (FR-EXP-06). Desktops keep exporting.
  static bool get _pausesInBackground => Platform.isIOS || Platform.isAndroid;

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (_progress == null || !_pausesInBackground) return;
    final paused = state != AppLifecycleState.resumed;
    if (paused == _paused) return;
    ref.read(projectGatewayProvider).setExportPaused(paused);
    setState(() => _paused = paused);
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
        unawaited(_saveCaptions(l10n));
        unawaited(_saveCover(l10n));
      case TaskEvent_Cancelled():
        setState(() => _progress = null);
      case TaskEvent_Failed(:final code):
        setState(() => _progress = null);
        showToast(context, errorMessage(l10n, EngineFailure(code: code)));
    }
  }

  /// The video's path without its extension: side files share its name.
  String? get _base {
    final path = _savedPath;
    if (path == null) return null;
    final dot = path.lastIndexOf('.');
    return dot < 0 ? path : path.substring(0, dot);
  }

  /// The `.srt` sits next to the video, with the same name.
  Future<void> _saveCaptions(AppLocalizations l10n) async {
    final base = _base;
    if (!_srt || base == null) return;
    try {
      await ref.read(projectGatewayProvider).writeSrt('$base.srt');
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  /// The cover sits next to the video: `<name>-cover.jpg`.
  Future<void> _saveCover(AppLocalizations l10n) async {
    final base = _base;
    final coverUs = _coverUs;
    if (coverUs == null || base == null) return;
    try {
      await ref
          .read(projectGatewayProvider)
          .writeCover('$base-cover.jpg', coverUs);
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  /// Turns the cover on at the frame the preview shows.
  void _toggleCover(bool on) {
    final transport = ref.read(previewTransportProvider);
    final at = transport.positionUs.clamp(0, _lastFrameUs);
    setState(() => _coverUs = on ? at : null);
  }

  int get _lastFrameUs => widget.durationUs <= 0 ? 0 : widget.durationUs - 1;

  /// Moving the cover shows that frame in the preview.
  void _moveCover(double value) {
    final at = value.round();
    ref.read(previewTransportProvider).seek(at);
    setState(() => _coverUs = at);
  }

  Future<void> _start() async {
    final l10n = AppLocalizations.of(context);
    final path = await _choose();
    if (path == null || !mounted) return;
    setState(() {
      _progress = 0;
      _paused = false;
    });
    final gateway = ref.read(projectGatewayProvider);
    _savedPath = path;
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
    final coverUs = _coverUs;
    final blocked = switch (widget.durationUs) {
      0 => l10n.exportEmpty,
      > _maxDurationUs => l10n.exportTooLong,
      _ => null,
    };
    if (progress != null) {
      return TaskProgress(
        label: _paused
            ? l10n.exportPaused
            : l10n.exportProgress((progress * 100).round()),
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
            options: {
              ..._widths,
              if (ref.read(projectGatewayProvider).exportSupports4k())
                _uhdWidth: '4K',
            },
            selected: _width,
            onSelected: (value) => setState(() => _width = value),
          ),
          _choice(
            label: l10n.exportFrameRate,
            options: {for (final fps in _frameRates) fps: '$fps'},
            selected: _fps,
            onSelected: (value) => setState(() => _fps = value),
          ),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l10n.exportSrt),
            value: _srt,
            onChanged: (value) => setState(() => _srt = value),
          ),
          SwitchListTile(
            key: const ValueKey('export-cover'),
            contentPadding: EdgeInsets.zero,
            title: Text(l10n.exportCover),
            subtitle: Text(
              coverUs == null
                  ? l10n.exportCoverHint
                  : l10n.exportCoverAt(formatTimecode(coverUs)),
            ),
            value: coverUs != null,
            onChanged: blocked == null ? _toggleCover : null,
          ),
          if (coverUs != null)
            Slider(
              key: const ValueKey('export-cover-time'),
              value: coverUs.toDouble(),
              max: _lastFrameUs.toDouble(),
              label: formatTimecode(coverUs),
              onChanged: _lastFrameUs > 0 ? _moveCover : null,
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
