import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/motion.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../audio/audio_panel.dart';
import '../captions/captions_panel.dart';
import '../clips/clips_panel.dart';
import '../enhance/animate_sheet.dart';
import '../enhance/enhance_panel.dart';
import '../enhance/path_bar.dart';
import '../enhance/overlay_selection.dart';
import '../export/export_panel.dart';
import '../project/project_dialogs.dart';
import '../preview/preview_controller.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';
import '../timeline/filmstrips.dart';
import '../timeline/timeline_layout.dart';
import '../timeline/timeline_view.dart';
import '../timeline/waveforms.dart';
import 'editor_section.dart';

class EditorTopBar extends StatelessWidget {
  const EditorTopBar({
    super.key,
    required this.title,
    required this.save,
    required this.onRename,
    required this.onClose,
    this.onHistory,
  });

  final String title;
  final SaveStateView save;
  final VoidCallback onRename;
  final VoidCallback onClose;
  final VoidCallback? onHistory;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final text = Theme.of(context).textTheme;
    return SizedBox(
      height: ShortedSize.topBar,
      child: Row(
        children: [
          ShortedIconButton(
            icon: Icons.close,
            tooltip: l10n.closeProject,
            onPressed: onClose,
          ),
          Expanded(
            child: InkWell(
              onTap: onRename,
              child: Text(
                title,
                overflow: TextOverflow.ellipsis,
                style: text.titleMedium,
              ),
            ),
          ),
          _SavedDot(save),
          const SizedBox(width: ShortedSpace.xs),
          Flexible(
            child: Text(
              switch (save) {
                SaveStateView.saving => l10n.saving,
                SaveStateView.saved => l10n.saved,
                SaveStateView.failed => l10n.saveFailed,
              },
              overflow: TextOverflow.ellipsis,
              style: text.labelMedium?.copyWith(color: ShortedColors.textMuted),
            ),
          ),
          if (onHistory != null)
            ShortedIconButton(
              icon: Icons.history,
              tooltip: l10n.versionHistory,
              onPressed: onHistory,
            ),
          const SizedBox(width: ShortedSpace.s),
        ],
      ),
    );
  }
}

class _SavedDot extends StatelessWidget {
  const _SavedDot(this.save);

  final SaveStateView save;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 6,
      height: 6,
      decoration: BoxDecoration(
        color: switch (save) {
          SaveStateView.saving => ShortedColors.attention,
          SaveStateView.saved => ShortedColors.accent,
          SaveStateView.failed => ShortedColors.danger,
        },
        shape: BoxShape.circle,
      ),
    );
  }
}

/// The engine's frames, in a texture. Nothing here may draw video content (ADR-0003).
class PreviewArea extends ConsumerStatefulWidget {
  const PreviewArea({super.key, this.projectId, this.playheadUs});

  /// With both, the selected overlay is outlined and a tap selects one.
  final String? projectId;
  final ValueNotifier<int>? playheadUs;

  @visibleForTesting
  static const cropKey = ValueKey('preview-crop');

  @override
  ConsumerState<PreviewArea> createState() => _PreviewAreaState();
}

class _PreviewAreaState extends ConsumerState<PreviewArea> {
  int? _textureId;
  late final PreviewTransport _transport = ref.read(previewTransportProvider);
  double _pinch = 1;

  @override
  void initState() {
    super.initState();
    unawaited(
      _transport.startTexture().then((id) {
        if (mounted) setState(() => _textureId = id);
      }),
    );
  }

  @override
  void dispose() {
    unawaited(_transport.stopTexture());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final id = _textureId;
    final crop = ref.watch(cropTargetProvider);
    final picture = ClipRRect(
      borderRadius: BorderRadius.circular(ShortedRadius.m),
      child: ColoredBox(
        color: ShortedColors.bg,
        child: id == null ? null : Texture(textureId: id),
      ),
    );
    return Padding(
      padding: const EdgeInsets.all(ShortedSpace.s),
      child: Center(
        child: AspectRatio(
          aspectRatio: 9 / 16,
          child: crop == null
              ? _withSelection(picture)
              : LayoutBuilder(
                  builder: (context, box) => GestureDetector(
                    key: PreviewArea.cropKey,
                    onScaleStart: (_) => _pinch = 1,
                    onScaleUpdate: (details) {
                      ref
                          .read(projectGatewayProvider)
                          .moveCrop(
                            crop.clipId,
                            details.focalPointDelta.dx / box.maxWidth,
                            details.focalPointDelta.dy / box.maxHeight,
                            details.scale / _pinch,
                          );
                      _pinch = details.scale;
                    },
                    onScaleEnd: (_) => _commit(crop),
                    child: DecoratedBox(
                      position: DecorationPosition.foreground,
                      decoration: BoxDecoration(
                        border: Border.all(
                          color: ShortedColors.accent,
                          width: 2,
                        ),
                        borderRadius: BorderRadius.circular(ShortedRadius.m),
                      ),
                      child: picture,
                    ),
                  ),
                ),
        ),
      ),
    );
  }

  Widget _withSelection(Widget picture) {
    final projectId = widget.projectId;
    final playheadUs = widget.playheadUs;
    if (projectId == null || playheadUs == null) return picture;
    return Stack(
      fit: StackFit.expand,
      children: [
        picture,
        OverlaySelection(projectId: projectId, playheadUs: playheadUs),
      ],
    );
  }

  Future<void> _commit(({String projectId, String clipId}) crop) async {
    final l10n = AppLocalizations.of(context);
    try {
      await ref
          .read(editorProvider(crop.projectId).notifier)
          .commitCrop(crop.clipId);
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }
}

class TransportBar extends ConsumerStatefulWidget {
  const TransportBar({
    super.key,
    required this.durationUs,
    required this.playheadUs,
    required this.onUndo,
    required this.onRedo,
  });

  final int durationUs;
  final ValueNotifier<int> playheadUs;
  final VoidCallback? onUndo;
  final VoidCallback? onRedo;

  @override
  ConsumerState<TransportBar> createState() => TransportBarState();
}

class TransportBarState extends ConsumerState<TransportBar>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_follow);
  late final PreviewTransport _transport = ref.read(previewTransportProvider);
  bool _following = false;

  @override
  void initState() {
    super.initState();
    widget.playheadUs.addListener(_onPlayhead);
  }

  @override
  void dispose() {
    widget.playheadUs.removeListener(_onPlayhead);
    _ticker.dispose();
    super.dispose();
  }

  /// A scrub moves the engine; a change coming from the engine must not go back to it.
  void _onPlayhead() {
    if (!_following) _transport.seek(widget.playheadUs.value);
  }

  void _follow(Duration _) {
    _following = true;
    widget.playheadUs.value = _transport.positionUs;
    _following = false;
    if (!_transport.isPlaying) {
      _ticker.stop();
      setState(() {});
    }
  }

  void pause() {
    if (_transport.isPlaying) toggle();
  }

  void toggle() {
    if (_transport.isPlaying) {
      _transport.pause();
      _ticker.stop();
    } else {
      _transport.play();
      _ticker.start();
    }
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    ref.listen(recordingVoiceProvider, (_, recording) {
      if (recording != _transport.isPlaying) toggle();
    });
    return SizedBox(
      height: ShortedSize.transport,
      child: Row(
        children: [
          ShortedIconButton(
            icon: _transport.isPlaying ? Icons.pause : Icons.play_arrow,
            tooltip: l10n.playPause,
            onPressed: widget.durationUs > 0 ? toggle : null,
          ),
          const SizedBox(width: ShortedSpace.s),
          Expanded(
            child: ValueListenableBuilder(
              valueListenable: widget.playheadUs,
              builder: (context, position, child) => Text(
                '${formatTimecode(position)} / ${formatTimecode(widget.durationUs)}',
                overflow: TextOverflow.ellipsis,
              ),
            ),
          ),
          ShortedIconButton(
            icon: Icons.undo,
            tooltip: l10n.undo,
            onPressed: widget.onUndo,
          ),
          ShortedIconButton(
            icon: Icons.redo,
            tooltip: l10n.redo,
            onPressed: widget.onRedo,
          ),
        ],
      ),
    );
  }
}

class TimelineArea extends ConsumerStatefulWidget {
  const TimelineArea({
    super.key,
    this.zoomRequests,
    this.collapsed = false,
    this.onToggleExpanded,
    required this.height,
    required this.view,
    required this.playheadUs,
  });

  final double height;
  final Stream<double>? zoomRequests;
  final bool collapsed;

  /// Offered on phones, where the timeline folds.
  final VoidCallback? onToggleExpanded;
  final ProjectView view;
  final ValueNotifier<int> playheadUs;

  @override
  ConsumerState<TimelineArea> createState() => _TimelineAreaState();
}

class _TimelineAreaState extends ConsumerState<TimelineArea> {
  (int, int)? _range;

  /// A selected sound: its tools show whatever the section.
  String? _audioItem;

  Future<void> _trackMenu(int index, Offset position) async {
    final l10n = AppLocalizations.of(context);
    final track = widget.view.tracks[index];
    final id = track.id;
    final isSound =
        track.kind == TrackKindView.audio ||
        track.kind == TrackKindView.voiceOver;
    final users = [
      for (final other in widget.view.tracks)
        if (other.id != null) other.id,
    ];
    final userIndex = users.indexOf(id);
    final area = Overlay.of(context).context.findRenderObject()! as RenderBox;
    final choice = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(
        position & Size.zero,
        Offset.zero & area.size,
      ),
      items: [
        if (isSound || track.kind == TrackKindView.video)
          PopupMenuItem(
            value: 'mute',
            child: Text(track.muted ? l10n.unmute : l10n.mute),
          ),
        PopupMenuItem(
          value: 'lock',
          child: Text(track.locked ? l10n.unlockTrack : l10n.lockTrack),
        ),
        PopupMenuItem(
          value: 'hide',
          child: Text(track.hidden ? l10n.showTrack : l10n.hideTrack),
        ),
        if (id != null) ...[
          if (userIndex > 0)
            PopupMenuItem(value: 'up', child: Text(l10n.moveTrackUp)),
          if (userIndex < users.length - 1)
            PopupMenuItem(value: 'down', child: Text(l10n.moveTrackDown)),
          PopupMenuItem(value: 'rename', child: Text(l10n.renameTrack)),
          PopupMenuItem(value: 'remove', child: Text(l10n.removeTrack)),
        ],
      ],
    );
    if (!mounted || choice == null) return;
    switch (choice) {
      case 'mute':
        await _guard(
          () => _editor.apply(
            (gateway) => gateway.setTrackFlags(track, muted: !track.muted),
          ),
        );
      case 'lock':
        await _guard(
          () => _editor.apply(
            (gateway) => gateway.setTrackFlags(track, locked: !track.locked),
          ),
        );
      case 'hide':
        await _guard(
          () => _editor.apply(
            (gateway) => gateway.setTrackFlags(track, hidden: !track.hidden),
          ),
        );
      case 'up' when id != null:
        await _guard(
          () => _editor.apply((g) => g.reorderAudioTrack(id, userIndex - 1)),
        );
      case 'down' when id != null:
        await _guard(
          () => _editor.apply((g) => g.reorderAudioTrack(id, userIndex + 1)),
        );
      case 'rename':
        final name = await askTitle(context, track.name ?? '');
        if (name != null && id != null) {
          await _guard(
            () => _editor.apply((g) => g.renameAudioTrack(id, name)),
          );
        }
      case 'remove':
        if (id == null) return;
        if (track.items.isNotEmpty && !await _confirmRemove()) return;
        await _guard(() => _editor.apply((g) => g.removeAudioTrack(id)));
    }
  }

  Future<bool> _confirmRemove() async {
    final l10n = AppLocalizations.of(context);
    final sure = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l10n.removeTrack),
        content: Text(l10n.removeTrackBody),
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
    return sure ?? false;
  }

  /// The selected clip, for the tools. Other tracks get their tools with their tickets.
  TimelineItem? _clip;

  late final _filmstrips = Filmstrips(
    ref.read(projectGatewayProvider).filmstrip,
  );

  late final _waveforms = Waveforms(ref.read(projectGatewayProvider).peaks);

  @override
  void dispose() {
    _filmstrips.dispose();
    _waveforms.dispose();
    super.dispose();
  }

  EditorController get _editor =>
      ref.read(editorProvider(widget.view.id).notifier);

  Future<void> _guard(Future<void> Function() action) async {
    final l10n = AppLocalizations.of(context);
    try {
      await action();
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  Future<void> _onRange(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  ) async {
    setState(() => _range = null);
    await _guard(() => _editor.apply(action));
  }

  bool get _cropping => ref.read(cropTargetProvider) != null;

  void _stopCropping() => ref.read(cropTargetProvider.notifier).set(null);

  /// Reframing happens on the picture of the clip, so the playhead goes there.
  void _toggleCrop(TimelineItem clip) {
    if (_cropping) {
      _stopCropping();
      return;
    }
    final playhead = widget.playheadUs.value;
    if (playhead < clip.startUs || playhead >= clip.endUs) {
      widget.playheadUs.value = clip.startUs;
    }
    ref.read(cropTargetProvider.notifier).set((
      projectId: widget.view.id,
      clipId: clip.id,
    ));
  }

  /// Null unless the clip is a still image; otherwise whether its slow zoom is on.
  bool? _slowZoom(TimelineItem clip) {
    for (final track in widget.view.tracks) {
      for (final item in track.items) {
        if (item.id == clip.id) return item.slowZoom;
      }
    }
    return null;
  }

  bool _isSound(TimelineItem item) => widget.view.tracks.any(
    (track) =>
        (track.kind == TrackKindView.audio ||
            track.kind == TrackKindView.voiceOver) &&
        track.items.any((other) => other.id == item.id),
  );

  OverlayView? _overlay(String? id) {
    for (final overlay in widget.view.overlays) {
      if (overlay.id == id) return overlay;
    }
    return null;
  }

  /// The easing to the next keys, and delete (T-0707).
  Widget _keyframeTools(
    AppLocalizations l10n,
    OverlayView overlay,
    KeyframeView keyframe,
  ) {
    final time = keyframe.timeUs.toInt();
    void clear() => ref.read(selectedKeyframeProvider.notifier).set(null);
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(
        children: [
          for (final (easing, label) in [
            (EasingView.linear, l10n.easingLinear),
            (EasingView.easeIn, l10n.easingIn),
            (EasingView.easeOut, l10n.easingOut),
            (EasingView.easeInOut, l10n.easingInOut),
            (EasingView.spring, l10n.easingSpring),
          ])
            Padding(
              padding: const EdgeInsets.only(right: ShortedSpace.xs),
              child: ShortedChip(
                label: label,
                selected: keyframe.easing == easing,
                onTap: () => _guard(
                  () => _editor.apply(
                    (g) => g.setKeyframeEasing(overlay.id, time, easing),
                  ),
                ),
              ),
            ),
          ShortedIconButton(
            icon: Icons.delete_outline,
            tooltip: l10n.deleteKeyframe,
            onPressed: () {
              clear();
              _guard(
                () =>
                    _editor.apply((g) => g.removeKeyframesAt(overlay.id, time)),
              );
            },
          ),
          ShortedIconButton(
            icon: Icons.close,
            tooltip: l10n.cancel,
            onPressed: clear,
          ),
        ],
      ),
    );
  }

  Widget _overlayTools(AppLocalizations l10n, OverlayView overlay) {
    Future<void> run(
      Future<ProjectView> Function(ProjectGateway gateway) action,
    ) => _guard(() => _editor.apply(action));
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        ShortedIconButton(
          icon: Icons.animation,
          tooltip: l10n.animate,
          onPressed: overlay.locked
              ? null
              : () => showAnimateSheet(
                  context,
                  projectId: widget.view.id,
                  overlayId: overlay.id,
                ),
        ),
        ShortedIconButton(
          icon: Icons.gesture,
          tooltip: l10n.path,
          onPressed: overlay.locked
              ? null
              : () => ref.read(pathEditingProvider.notifier).set(overlay.id),
        ),
        ShortedIconButton(
          icon: Icons.flip_to_front,
          tooltip: l10n.bringForward,
          onPressed: () =>
              run((g) => g.shiftOverlayLayer(overlay.id, forward: true)),
        ),
        ShortedIconButton(
          icon: Icons.flip_to_back,
          tooltip: l10n.sendBackward,
          onPressed: () =>
              run((g) => g.shiftOverlayLayer(overlay.id, forward: false)),
        ),
        ShortedIconButton(
          icon: overlay.locked ? Icons.lock : Icons.lock_open,
          tooltip: overlay.locked ? l10n.unlockTrack : l10n.lockTrack,
          onPressed: () => run(
            (g) => g.setOverlayFlags(
              overlay.id,
              locked: !overlay.locked,
              hidden: overlay.hidden,
            ),
          ),
        ),
        ShortedIconButton(
          icon: overlay.hidden ? Icons.visibility_off : Icons.visibility,
          tooltip: overlay.hidden ? l10n.showTrack : l10n.hideTrack,
          onPressed: () => run(
            (g) => g.setOverlayFlags(
              overlay.id,
              locked: overlay.locked,
              hidden: !overlay.hidden,
            ),
          ),
        ),
        ShortedIconButton(
          icon: Icons.copy_outlined,
          tooltip: l10n.duplicate,
          onPressed: () => run((g) => g.duplicateOverlay(overlay.id)),
        ),
        ShortedIconButton(
          icon: Icons.delete_outline,
          tooltip: l10n.delete,
          onPressed: overlay.locked
              ? null
              : () {
                  ref.read(selectedOverlayProvider.notifier).set(null);
                  run((g) => g.deleteOverlay(overlay.id));
                },
        ),
      ],
    );
  }

  bool _isClip(TimelineItem item) => widget.view.tracks.any(
    (track) =>
        track.kind == TrackKindView.video &&
        track.items.any((other) => other.id == item.id),
  );

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final tracks = [
      for (final track in widget.view.tracks)
        TimelineTrack(
          kind: switch (track.kind) {
            TrackKindView.widgets => TrackKind.widgets,
            TrackKindView.video => TrackKind.video,
            TrackKindView.captions => TrackKind.captions,
            TrackKindView.voiceOver => TrackKind.voiceOver,
            TrackKindView.audio => TrackKind.audio,
          },
          label:
              track.name ??
              switch (track.kind) {
                TrackKindView.widgets => l10n.trackWidgets,
                TrackKindView.video => l10n.trackVideo,
                TrackKindView.captions => l10n.trackCaptions,
                _ => l10n.trackVoiceOver,
              },
          id: track.id,
          muted: track.muted,
          locked: track.locked,
          hidden: track.hidden,
          items: [
            for (final item in track.items)
              TimelineItem(
                id: item.id,
                startUs: item.startUs.toInt(),
                endUs: item.endUs.toInt(),
                mediaId: item.mediaId,
                sourceInUs: item.sourceInUs.toInt(),
                speed: item.speed,
                locked: _overlay(item.id)?.locked ?? false,
                animated: _overlay(item.id)?.hasAnimation ?? false,
                keyframes: [
                  for (final key in _overlay(item.id)?.keyframes ?? const [])
                    key.timeUs.toInt(),
                ],
              ),
          ],
        ),
    ];
    for (final track in widget.view.tracks) {
      for (final item in track.items) {
        final mediaId = item.mediaId;
        if (mediaId == null) continue;
        if (track.kind == TrackKindView.video) {
          _filmstrips.request(mediaId);
        } else {
          _waveforms.request(mediaId);
        }
      }
    }
    final clip = _clip;
    final range = _range;
    final selectedOverlay = ref.watch(selectedOverlayProvider);
    final overlay = _overlay(selectedOverlay);
    final picked = ref.watch(selectedKeyframeProvider);
    final pathEditing = ref.watch(pathEditingProvider);
    KeyframeView? keyframe;
    if (overlay != null && picked?.overlayId == overlay.id) {
      for (final key in overlay.keyframes) {
        if (key.timeUs.toInt() == picked?.timeUs) keyframe = key;
      }
    }
    final cropping = ref.watch(cropTargetProvider) != null;
    ref.watch(selectedClipProvider);
    final slowZoom = clip == null ? null : _slowZoom(clip);
    return AnimatedContainer(
      duration: motionDuration(context, ShortedMotion.section),
      curve: Curves.easeOutCubic,
      height: widget.height,
      child: Column(
        children: [
          if (widget.onToggleExpanded != null)
            Align(
              alignment: Alignment.centerRight,
              child: TextButton(
                onPressed: widget.onToggleExpanded,
                child: Text(
                  widget.collapsed ? l10n.allTracks : l10n.collapseTimeline,
                ),
              ),
            ),
          if (range != null)
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                ShortedIconButton(
                  icon: Icons.vertical_split_outlined,
                  tooltip: l10n.splitAll,
                  onPressed: () =>
                      _onRange((gateway) => gateway.splitAllAt(range.$1)),
                ),
                ShortedIconButton(
                  icon: Icons.delete_sweep_outlined,
                  tooltip: l10n.deleteRange,
                  onPressed: () => _onRange(
                    (gateway) => gateway.deleteRange(range.$1, range.$2),
                  ),
                ),
                ShortedIconButton(
                  icon: Icons.volume_off_outlined,
                  tooltip: l10n.muteRange,
                  onPressed: () => _onRange(
                    (gateway) => gateway.muteRange(range.$1, range.$2),
                  ),
                ),
                ShortedIconButton(
                  icon: Icons.close,
                  tooltip: l10n.clearRange,
                  onPressed: () => setState(() => _range = null),
                ),
              ],
            )
          else if (overlay != null && pathEditing == overlay.id)
            PathBar(
              overlay: overlay,
              run: (action) => _guard(() => _editor.apply(action)),
            )
          else if (overlay != null && keyframe != null)
            _keyframeTools(l10n, overlay, keyframe)
          else if (overlay != null)
            _overlayTools(l10n, overlay)
          else if (_audioItem case final sound?)
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                ShortedIconButton(
                  icon: Icons.delete_outline,
                  tooltip: l10n.delete,
                  onPressed: () {
                    setState(() => _audioItem = null);
                    _guard(() => _editor.apply((g) => g.deleteAudio(sound)));
                  },
                ),
              ],
            )
          else if (clip != null)
            Row(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                ShortedIconButton(
                  icon: Icons.content_cut,
                  tooltip: l10n.split,
                  onPressed: () => _guard(
                    () => _editor.splitClip(clip.id, widget.playheadUs.value),
                  ),
                ),
                ShortedIconButton(
                  icon: Icons.copy_outlined,
                  tooltip: l10n.duplicate,
                  onPressed: () => _guard(() => _editor.duplicateClip(clip.id)),
                ),
                if (slowZoom != null)
                  ShortedIconButton(
                    icon: slowZoom ? Icons.zoom_in_map : Icons.zoom_out_map,
                    tooltip: l10n.slowZoom,
                    onPressed: () =>
                        _guard(() => _editor.toggleSlowZoom(clip.id)),
                  ),
                ShortedIconButton(
                  icon: cropping ? Icons.check : Icons.crop,
                  tooltip: l10n.crop,
                  onPressed: () => _toggleCrop(clip),
                ),
                if (cropping)
                  ShortedIconButton(
                    icon: Icons.restart_alt,
                    tooltip: l10n.resetCrop,
                    onPressed: () => _guard(() => _editor.resetCrop(clip.id)),
                  ),
                ShortedIconButton(
                  icon: Icons.delete_outline,
                  tooltip: l10n.delete,
                  onPressed: () {
                    _stopCropping();
                    setState(() => _clip = null);
                    ref.read(selectedClipProvider.notifier).set(null);
                    _guard(() => _editor.deleteClip(clip.id));
                  },
                ),
              ],
            ),
          Expanded(
            child: TimelineView(
              tracks: tracks,
              durationUs: widget.view.durationUs.toInt(),
              playheadUs: widget.playheadUs,
              filmstrips: _filmstrips,
              waveforms: _waveforms,
              collapsed: widget.collapsed,
              onTrackMenu: _trackMenu,
              onAudioMove: (item, startUs, track) => _guard(
                () => _editor.apply(
                  (gateway) => gateway.moveAudio(item.id, startUs, track.id),
                ),
              ),
              onAudioTrim: (item, start, deltaUs) => _guard(
                () => _editor.apply(
                  (gateway) => gateway.trimAudio(
                    item.id,
                    start: start,
                    deltaUs: deltaUs,
                  ),
                ),
              ),
              selectedId: selectedOverlay,
              selectedKeyframe: keyframe?.timeUs.toInt(),
              onKeyframeTap: (item, timeUs) {
                ref.read(selectedOverlayProvider.notifier).set(item.id);
                ref.read(selectedKeyframeProvider.notifier).set((
                  overlayId: item.id,
                  timeUs: timeUs,
                ));
              },
              onKeyframeMove: (item, fromUs, toUs) {
                ref.read(selectedKeyframeProvider.notifier).set((
                  overlayId: item.id,
                  timeUs: toUs,
                ));
                _guard(
                  () => _editor.apply(
                    (g) => g.moveKeyframes(item.id, fromUs, toUs),
                  ),
                );
              },
              onOverlayTiming: (item, startUs, endUs) => _guard(
                () => _editor.apply(
                  (gateway) =>
                      gateway.setOverlayTiming(item.id, startUs, endUs),
                ),
              ),
              rangeUs: range,
              onRange: (next) => setState(() => _range = next),
              zoomRequests: widget.zoomRequests,
              onSelect: (item) {
                _stopCropping();
                setState(() {
                  _clip = item != null && _isClip(item) ? item : null;
                  _audioItem = item != null && _isSound(item) ? item.id : null;
                });
                ref.read(selectedClipProvider.notifier).set(_clip?.id);
                ref
                    .read(selectedOverlayProvider.notifier)
                    .set(
                      item != null && _overlay(item.id) != null
                          ? item.id
                          : null,
                    );
              },
              onTrim: (item, start, deltaUs) => _guard(
                () => _editor.trimClip(item.id, start: start, deltaUs: deltaUs),
              ),
              onMove: (item, to) => _guard(() => _editor.moveClip(item.id, to)),
            ),
          ),
        ],
      ),
    );
  }
}

/// Tools of the active section; only this part animates when the section changes.
class SectionPanel extends StatelessWidget {
  const SectionPanel({
    super.key,
    required this.section,
    required this.projectId,
    required this.view,
  });

  final EditorSection section;
  final String projectId;
  final ProjectView view;

  @override
  Widget build(BuildContext context) {
    // Only the panel animates: the preview and timeline sit outside it and never move.
    return AnimatedSwitcher(
      duration: motionDuration(context, ShortedMotion.section),
      transitionBuilder: fadeThrough,
      child: switch (section) {
        EditorSection.clips => ClipsPanel(
          key: const ValueKey(EditorSection.clips),
          projectId: projectId,
          media: view.media,
        ),
        EditorSection.audio => AudioPanel(
          key: const ValueKey(EditorSection.audio),
          view: view,
        ),
        EditorSection.captions => CaptionsPanel(
          key: const ValueKey(EditorSection.captions),
          view: view,
        ),
        EditorSection.enhance => EnhancePanel(
          key: const ValueKey(EditorSection.enhance),
          view: view,
        ),
        EditorSection.export => ExportPanel(
          key: const ValueKey(EditorSection.export),
          durationUs: view.durationUs.toInt(),
        ),
      },
    );
  }
}

class SectionBar extends StatelessWidget {
  const SectionBar({super.key, required this.selected, required this.onSelect});

  final EditorSection selected;
  final ValueChanged<EditorSection> onSelect;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return NavigationBar(
      height: ShortedSize.sectionBar,
      backgroundColor: ShortedColors.surface,
      indicatorColor: ShortedColors.surfaceRaised,
      selectedIndex: selected.index,
      onDestinationSelected: (i) => onSelect(EditorSection.values[i]),
      destinations: [
        for (final section in EditorSection.values)
          NavigationDestination(
            icon: Icon(section.icon),
            label: section.label(l10n),
          ),
      ],
    );
  }
}

class SectionRail extends StatelessWidget {
  const SectionRail({
    super.key,
    required this.selected,
    required this.onSelect,
  });

  final EditorSection selected;
  final ValueChanged<EditorSection> onSelect;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return NavigationRail(
      backgroundColor: ShortedColors.surface,
      indicatorColor: ShortedColors.surfaceRaised,
      labelType: NavigationRailLabelType.all,
      selectedIndex: selected.index,
      onDestinationSelected: (i) => onSelect(EditorSection.values[i]),
      destinations: [
        for (final section in EditorSection.values)
          NavigationRailDestination(
            icon: Icon(section.icon),
            label: Text(section.label(l10n)),
          ),
      ],
    );
  }
}

class Inspector extends StatelessWidget {
  const Inspector({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Container(
      width: 280,
      color: ShortedColors.surface,
      padding: const EdgeInsets.all(ShortedSpace.l),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            l10n.inspectorTitle,
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: ShortedSpace.s),
          Text(
            l10n.nothingSelected,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        ],
      ),
    );
  }
}
