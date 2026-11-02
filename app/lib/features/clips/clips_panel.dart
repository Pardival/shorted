import 'dart:async';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/media.dart';
import '../../bridge/error.dart';
import '../../bridge/runtime.dart';
import '../../design_system/components.dart';
import '../../design_system/task_progress.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../settings/ai_settings_section.dart';
import 'ai_consent.dart';
import 'arrange_review.dart';
import '../project/project_providers.dart';
import '../timeline/timeline_layout.dart';

const _mediaTypes = XTypeGroup(
  label: 'media',
  extensions: [
    'mov',
    'mp4',
    'm4v',
    'webm',
    'jpg',
    'jpeg',
    'png',
    'heic',
    'webp',
    'gif',
  ],
);

/// Import and manual order (S10, S10b). Smart arrange joins it in M4.
/// On phones media is always copied in, so there is nothing to collect.
bool get isDesktop =>
    defaultTargetPlatform == TargetPlatform.macOS ||
    defaultTargetPlatform == TargetPlatform.windows ||
    defaultTargetPlatform == TargetPlatform.linux;

class ClipsPanel extends ConsumerStatefulWidget {
  const ClipsPanel({super.key, required this.projectId, required this.media});

  final String projectId;
  final List<MediaItemView> media;

  @override
  ConsumerState<ClipsPanel> createState() => _ClipsPanelState();
}

class _ClipsPanelState extends ConsumerState<ClipsPanel> {
  /// In tap order: this is the order on the timeline.
  final _picked = <String>[];
  var _dragging = false;

  /// Smart arrange (FR-ARR-06): its progress while it runs, and how to cancel it.
  double? _arranging;
  BigInt? _arrangeTask;
  StreamSubscription<TaskEvent>? _arrangeEvents;

  @override
  void dispose() {
    unawaited(_arrangeEvents?.cancel());
    super.dispose();
  }

  void _arrange() =>
      _follow(ref.read(projectGatewayProvider).analyseForArrange());

  /// AI arrangement (T-0806): the exact request is shown for consent, unless the user asked to
  /// be asked only once for this project.
  Future<void> _arrangeWithAi({String? instruction}) async {
    final l10n = AppLocalizations.of(context);
    final gateway = ref.read(projectGatewayProvider);
    final language = Localizations.localeOf(context).languageCode;
    // Consent is asked per kind of request (docs/07 §4): arranging, and free instructions.
    final kind = instruction == null
        ? widget.projectId
        : '${widget.projectId}:ask';
    try {
      final payload = await gateway.prepareAiArrangement(
        language,
        instruction: instruction,
      );
      if (!mounted) return;
      if (!gateway.aiConsentRemembered(kind)) {
        final consent = await askAiConsent(context, payload);
        if (!consent.send || !mounted) return;
        if (consent.remember) await gateway.rememberAiConsent(kind);
      }
      _follow(gateway.sendAiArrangement(), ai: true);
    } on Object catch (error) {
      if (mounted) showToast(context, connectionError(l10n, error));
    }
  }

  /// "Ask Shorted" (FR-AI-05): one instruction in the user's words, then the same consent,
  /// validation and review as an arrangement.
  Future<void> _ask() async {
    final instruction = await askShorted(context);
    if (instruction == null || !mounted) return;
    await _arrangeWithAi(instruction: instruction);
  }

  void _follow(Stream<TaskEvent> events, {bool ai = false}) {
    final l10n = AppLocalizations.of(context);
    setState(() => _arranging = 0);
    _arrangeEvents = events.listen((event) async {
      if (!mounted) return;
      switch (event) {
        case TaskEvent_Started(taskId: final id):
          _arrangeTask = id;
        case TaskEvent_Progress(:final fraction):
          setState(() => _arranging = fraction);
        case TaskEvent_Finished():
          setState(() => _arranging = null);
          if (ai && ref.read(projectGatewayProvider).aiArrangementFellBack()) {
            showToast(context, l10n.aiFellBack);
          }
          await _review();
        case TaskEvent_Cancelled():
          setState(() => _arranging = null);
        case TaskEvent_Failed(:final code):
          setState(() => _arranging = null);
          showToast(context, connectionError(l10n, EngineFailure(code: code)));
      }
    });
  }

  Future<void> _review() async {
    final l10n = AppLocalizations.of(context);
    try {
      final review = await _editor.arrange();
      if (!mounted) return;
      final outcome = await Navigator.of(context).push<ArrangeOutcome>(
        MaterialPageRoute(
          fullscreenDialog: true,
          builder: (_) =>
              ArrangeReview(projectId: widget.projectId, review: review),
        ),
      );
      if (outcome == ArrangeOutcome.adjust && mounted) {
        showToast(context, l10n.adjustHint);
      }
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  EditorController get _editor =>
      ref.read(editorProvider(widget.projectId).notifier);

  Future<void> _guard(Future<void> Function() action) async {
    final l10n = AppLocalizations.of(context);
    try {
      await action();
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  Future<void> _import(List<String> paths) async {
    if (paths.isEmpty) return;
    final l10n = AppLocalizations.of(context);
    await _guard(() async {
      final failed = await _editor.importFiles(paths);
      if (failed > 0 && mounted) showToast(context, l10n.importFailed(failed));
    });
  }

  Future<void> _pickFiles() async {
    final files = await openFiles(acceptedTypeGroups: const [_mediaTypes]);
    await _import([for (final file in files) file.path]);
  }

  Future<void> _place() => _guard(() async {
    await _editor.placeClips(List.of(_picked));
    if (mounted) setState(_picked.clear);
  });

  Future<void> _relink(MediaItemView item) async {
    final file = await openFile(acceptedTypeGroups: const [_mediaTypes]);
    if (file == null) return;
    await _guard(() => _editor.relink(item.id, file.path));
  }

  void _toggle(MediaItemView item) {
    setState(() {
      if (!_picked.remove(item.id)) _picked.add(item.id);
    });
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final media = [
      for (final item in widget.media)
        if (item.kind != MediaKindView.audio) item,
    ];
    return DropTarget(
      onDragEntered: (_) => setState(() => _dragging = true),
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (details) {
        setState(() => _dragging = false);
        _import([for (final file in details.files) file.path]);
      },
      child: ColoredBox(
        color: _dragging ? ShortedColors.surfaceRaised : Colors.transparent,
        child: Padding(
          padding: const EdgeInsets.all(ShortedSpace.m),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Buttons and media scroll together: a phone panel is short, and a squeezed row
              // of buttons wrapped their labels into ovals.
              Expanded(
                child: ListView(
                  children: [
                    SecondaryButton(
                      label: l10n.addMedia,
                      onPressed: _pickFiles,
                    ),
                    const SizedBox(height: ShortedSpace.s),
                    Row(
                      children: [
                        Expanded(
                          child: switch (_arranging) {
                            final progress? => TaskProgress(
                              label: l10n.arranging((progress * 100).round()),
                              fraction: progress,
                              cancelLabel: l10n.cancel,
                              onCancel: () {
                                final task = _arrangeTask;
                                if (task != null) {
                                  ref
                                      .read(projectGatewayProvider)
                                      .cancelTask(task);
                                }
                              },
                            ),
                            null => PrimaryButton(
                              label: l10n.smartArrange,
                              icon: Icons.auto_fix_high,
                              onPressed: _arrange,
                            ),
                          },
                        ),
                        if (_arranging == null)
                          PopupMenuButton<bool>(
                            icon: const Icon(Icons.auto_awesome),
                            tooltip: l10n.aiMenu,
                            onSelected: (ask) =>
                                ask ? _ask() : _arrangeWithAi(),
                            itemBuilder: (context) => [
                              PopupMenuItem(
                                value: false,
                                child: Text(l10n.arrangeWithAi),
                              ),
                              PopupMenuItem(
                                value: true,
                                child: Text(l10n.askShorted),
                              ),
                            ],
                          ),
                      ],
                    ),
                    if (isDesktop && media.isNotEmpty)
                      Align(
                        alignment: AlignmentDirectional.centerStart,
                        child: TextButton.icon(
                          icon: const Icon(Icons.drive_folder_upload_outlined),
                          label: Text(l10n.collectMedia),
                          onPressed: () => _guard(() async {
                            await _editor.apply(
                              (gateway) => gateway.collectMedia(),
                            );
                            if (context.mounted) {
                              showToast(context, l10n.mediaCollected);
                            }
                          }),
                        ),
                      ),
                    const SizedBox(height: ShortedSpace.s),
                    if (media.isEmpty)
                      Padding(
                        padding: const EdgeInsets.all(ShortedSpace.l),
                        child: Text(
                          l10n.dropMedia,
                          textAlign: TextAlign.center,
                          style: const TextStyle(
                            color: ShortedColors.textMuted,
                          ),
                        ),
                      ),
                    for (final item in media)
                      _MediaRow(
                        item: item,
                        order: _picked.contains(item.id)
                            ? _picked.indexOf(item.id) + 1
                            : null,
                        onTap: item.offline ? null : () => _toggle(item),
                        onRelink: () => _relink(item),
                      ),
                  ],
                ),
              ),
              if (_picked.isNotEmpty)
                PrimaryButton(
                  label: l10n.placeInOrder(_picked.length),
                  onPressed: _place,
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _MediaRow extends StatelessWidget {
  const _MediaRow({
    required this.item,
    required this.order,
    required this.onTap,
    required this.onRelink,
  });

  final MediaItemView item;
  final int? order;
  final VoidCallback? onTap;
  final VoidCallback onRelink;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final badge = order;
    return ListTile(
      dense: true,
      onTap: onTap,
      leading: badge == null
          ? Icon(
              item.kind == MediaKindView.video
                  ? Icons.movie_outlined
                  : Icons.image_outlined,
            )
          : ShortedChip(label: '$badge', selected: true),
      title: Text(item.name, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        item.offline
            ? l10n.mediaOffline
            : formatTimecode(item.durationUs.toInt()),
        style: TextStyle(
          color: item.offline
              ? ShortedColors.attention
              : ShortedColors.textMuted,
        ),
      ),
      trailing: item.offline
          ? TextButton(onPressed: onRelink, child: Text(l10n.relink))
          : null,
    );
  }
}
