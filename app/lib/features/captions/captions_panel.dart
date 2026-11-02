import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/projects.dart';
import '../../bridge/api/transcribe.dart';
import '../../bridge/error.dart';
import '../../bridge/runtime.dart';
import '../../design_system/components.dart';
import '../../design_system/task_progress.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

const _bytesPerMegabyte = 1000 * 1000;

/// Screen S30: the speech model, then the transcript, word by word.
class CaptionsPanel extends ConsumerStatefulWidget {
  const CaptionsPanel({super.key, required this.view});

  final ProjectView view;

  @override
  ConsumerState<CaptionsPanel> createState() => _CaptionsPanelState();
}

class _CaptionsPanelState extends ConsumerState<CaptionsPanel> {
  double? _progress;
  BigInt? _task;
  var _downloading = false;
  StreamSubscription<TaskEvent>? _events;

  ProjectGateway get _gateway => ref.read(projectGatewayProvider);
  EditorController get _editor =>
      ref.read(editorProvider(widget.view.id).notifier);

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }

  void _follow(Stream<TaskEvent> events, {required bool downloading}) {
    final l10n = AppLocalizations.of(context);
    setState(() {
      _progress = 0;
      _downloading = downloading;
    });
    _events = events.listen((event) async {
      if (!mounted) return;
      switch (event) {
        case TaskEvent_Started(taskId: final id):
          _task = id;
        case TaskEvent_Progress(:final fraction):
          setState(() => _progress = fraction);
        case TaskEvent_Finished():
          setState(() => _progress = null);
          if (!downloading) await _editor.reload();
        case TaskEvent_Cancelled():
          setState(() => _progress = null);
        case TaskEvent_Failed(:final code):
          setState(() => _progress = null);
          showToast(context, errorMessage(l10n, EngineFailure(code: code)));
      }
    });
  }

  Future<void> _run(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  ) async {
    final l10n = AppLocalizations.of(context);
    try {
      await _editor.apply(action);
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final progress = _progress;
    if (progress != null) {
      final percent = (progress * 100).round();
      return TaskProgress(
        label: _downloading
            ? l10n.downloadingModel(percent)
            : l10n.transcribing(percent),
        fraction: progress,
        cancelLabel: l10n.cancel,
        onCancel: () {
          final task = _task;
          if (task != null) _gateway.cancelTask(task);
        },
      );
    }
    final model = _gateway.speechModel();
    final transcripts = widget.view.transcripts;
    return ListView(
      padding: const EdgeInsets.all(ShortedSpace.m),
      children: [
        Text(
          l10n.speechStaysLocal,
          style: const TextStyle(color: ShortedColors.textMuted),
        ),
        const SizedBox(height: ShortedSpace.s),
        if (model.ready)
          PrimaryButton(
            label: l10n.transcribeNow,
            onPressed: () => _follow(_gateway.transcribe(), downloading: false),
          )
        else
          SecondaryButton(
            label: l10n.downloadSpeechModel(
              (model.bytes ~/ BigInt.from(_bytesPerMegabyte)).toInt(),
            ),
            onPressed: () =>
                _follow(_gateway.downloadSpeechModel(), downloading: true),
          ),
        const SizedBox(height: ShortedSpace.m),
        if (transcripts.isEmpty)
          Text(
            l10n.noTranscript,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        for (final transcript in transcripts)
          for (final segment in transcript.segments)
            _Phrase(transcript: transcript, segment: segment, run: _run),
      ],
    );
  }
}

class _Phrase extends StatelessWidget {
  const _Phrase({
    required this.transcript,
    required this.segment,
    required this.run,
  });

  final TranscriptView transcript;
  final SegmentView segment;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  Future<void> _edit(BuildContext context, WordView word) async {
    final text = await askTitle(context, word.text);
    if (text == null || text.isEmpty || text == word.text) return;
    await run(
      (gateway) => gateway.editWord(transcript, segment.id, word.index, text),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final hidden = segment.hidden;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: Wrap(
            spacing: ShortedSpace.xs,
            children: [
              for (final word in segment.words)
                if (!word.deleted)
                  Tooltip(
                    message: l10n.editWord,
                    child: InkWell(
                      onTap: () => _edit(context, word),
                      child: Padding(
                        padding: const EdgeInsets.all(ShortedSpace.xs),
                        child: Text(
                          word.text,
                          style: TextStyle(
                            color: hidden
                                ? ShortedColors.textMuted
                                : ShortedColors.text,
                          ),
                        ),
                      ),
                    ),
                  ),
            ],
          ),
        ),
        ShortedIconButton(
          icon: hidden ? Icons.visibility_off : Icons.visibility,
          tooltip: hidden ? l10n.showInCaptions : l10n.hideFromCaptions,
          onPressed: () => run(
            (gateway) =>
                gateway.hideSegment(transcript, segment.id, hidden: !hidden),
          ),
        ),
      ],
    );
  }
}
