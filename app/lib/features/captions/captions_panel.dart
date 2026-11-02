import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/projects.dart';
import '../../bridge/api/transcribe.dart';
import '../../bridge/error.dart';
import '../../bridge/runtime.dart';
import '../../design_system/components.dart';
import '../../design_system/task_progress.dart';
import '../../design_system/swatches.dart';
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
        const SizedBox(height: ShortedSpace.m),
        if (transcripts.isEmpty)
          Text(
            l10n.noTranscript,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        for (final transcript in transcripts)
          for (final segment in transcript.segments)
            _Phrase(transcript: transcript, segment: segment, run: _run),
        const SizedBox(height: ShortedSpace.m),
        _CaptionStyle(
          key: ValueKey(widget.view.captions.preset),
          captions: widget.view.captions,
          run: _run,
        ),
      ],
    );
  }
}

const _fonts = ['Space Grotesk', 'Inter'];
const _colours = [
  '#FFFFFFFF',
  '#FFE14DFF',
  '#C6F135FF',
  '#FF5C7AFF',
  '#4DD2FFFF',
];

String _presetName(AppLocalizations l10n, CaptionPresetView preset) =>
    switch (preset) {
      CaptionPresetView.wordByWord => l10n.presetWordByWord,
      CaptionPresetView.wordGroups => l10n.presetWordGroups,
      CaptionPresetView.sentence => l10n.presetSentence,
      CaptionPresetView.karaoke => l10n.presetKaraoke,
      CaptionPresetView.outline => l10n.presetOutline,
      CaptionPresetView.box => l10n.presetBox,
      CaptionPresetView.bold => l10n.presetBold,
      CaptionPresetView.minimal => l10n.presetMinimal,
    };

String _animationName(AppLocalizations l10n, CaptionAnimationView animation) =>
    switch (animation) {
      CaptionAnimationView.none => l10n.animationNone,
      CaptionAnimationView.pop => l10n.animationPop,
      CaptionAnimationView.fade => l10n.animationFade,
      CaptionAnimationView.typewriter => l10n.animationTypewriter,
    };

/// Sliders follow the finger locally; the engine hears once, when the finger lifts.
class _CaptionStyle extends StatefulWidget {
  const _CaptionStyle({super.key, required this.captions, required this.run});

  final CaptionsView captions;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  State<_CaptionStyle> createState() => _CaptionStyleState();
}

class _CaptionStyleState extends State<_CaptionStyle> {
  double? _size;
  double? _anchor;

  void _style({
    String? font,
    bool? uppercase,
    String? color,
    String? emphasisColor,
  }) {
    final captions = widget.captions;
    widget.run(
      (gateway) => gateway.setCaptionStyle(
        font: font ?? captions.font,
        size: _size ?? captions.size,
        uppercase: uppercase ?? captions.uppercase,
        color: color ?? captions.color,
        emphasisColor: emphasisColor ?? captions.emphasisColor,
        anchor: _anchor ?? captions.anchor,
      ),
    );
  }

  Widget _heading(String text) => Padding(
    padding: const EdgeInsets.only(
      top: ShortedSpace.m,
      bottom: ShortedSpace.xs,
    ),
    child: Text(text, style: const TextStyle(fontWeight: FontWeight.w600)),
  );

  Widget _swatches(String selected, ValueChanged<String> onTap) =>
      ColourSwatches(selected: selected, onTap: onTap, colours: _colours);

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final captions = widget.captions;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(l10n.captionsOn),
          value: captions.enabled,
          onChanged: (enabled) => widget.run(
            (gateway) => gateway.setCaptionsEnabled(enabled: enabled),
          ),
        ),
        _heading(l10n.captionStyle),
        Wrap(
          spacing: ShortedSpace.s,
          runSpacing: ShortedSpace.s,
          children: [
            for (final preset in CaptionPresetView.values)
              ShortedChip(
                label: _presetName(l10n, preset),
                selected: captions.preset == preset,
                onTap: () =>
                    widget.run((gateway) => gateway.setCaptionPreset(preset)),
              ),
          ],
        ),
        _heading(l10n.captionAnimation),
        Wrap(
          spacing: ShortedSpace.s,
          runSpacing: ShortedSpace.s,
          children: [
            for (final animation in CaptionAnimationView.values)
              ShortedChip(
                label: _animationName(l10n, animation),
                selected: captions.animation == animation,
                onTap: () => widget.run(
                  (gateway) => gateway.setCaptionAnimation(animation),
                ),
              ),
          ],
        ),
        _heading(l10n.captionFont),
        Wrap(
          spacing: ShortedSpace.s,
          children: [
            for (final font in _fonts)
              ShortedChip(
                label: font,
                selected: captions.font == font,
                onTap: () => _style(font: font),
              ),
          ],
        ),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(l10n.captionUppercase),
          value: captions.uppercase,
          onChanged: (uppercase) => _style(uppercase: uppercase),
        ),
        _heading(l10n.captionSize),
        Slider(
          min: 40,
          max: 160,
          value: (_size ?? captions.size).clamp(40, 160),
          label: l10n.captionSize,
          onChanged: (value) => setState(() => _size = value),
          onChangeEnd: (_) => _style(),
        ),
        _heading(l10n.captionPosition),
        Slider(
          min: 0.15,
          max: 0.75,
          value: (_anchor ?? captions.anchor).clamp(0.15, 0.75),
          label: l10n.captionPosition,
          onChanged: (value) => setState(() => _anchor = value),
          onChangeEnd: (_) => _style(),
        ),
        _heading(l10n.textColour),
        _swatches(captions.color, (color) => _style(color: color)),
        _heading(l10n.emphasisColour),
        _swatches(
          captions.emphasisColor,
          (color) => _style(emphasisColor: color),
        ),
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
                    message: l10n.toggleEmphasis,
                    child: InkWell(
                      onTap: () => run(
                        (gateway) => gateway.toggleEmphasis(
                          transcript,
                          segment.id,
                          word.index,
                        ),
                      ),
                      onLongPress: () => _edit(context, word),
                      child: Padding(
                        padding: const EdgeInsets.all(ShortedSpace.xs),
                        child: Text(
                          word.text,
                          style: TextStyle(
                            color: hidden
                                ? ShortedColors.textMuted
                                : word.emphasis
                                ? ShortedColors.accent
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
