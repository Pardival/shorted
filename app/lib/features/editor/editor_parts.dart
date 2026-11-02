import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/media.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../clips/clips_panel.dart';
import '../export/export_panel.dart';
import '../project/project_dialogs.dart';
import '../preview/preview_controller.dart';
import '../project/project_providers.dart';
import '../timeline/filmstrips.dart';
import '../timeline/timeline_layout.dart';
import '../timeline/timeline_view.dart';
import 'editor_section.dart';

class EditorTopBar extends StatelessWidget {
  const EditorTopBar({
    super.key,
    required this.title,
    required this.save,
    required this.onRename,
    required this.onClose,
    required this.onExport,
  });

  final String title;
  final SaveStateView save;
  final VoidCallback onRename;
  final VoidCallback onClose;
  final VoidCallback onExport;

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
          const SizedBox(width: ShortedSpace.m),
          PrimaryButton(
            label: l10n.sectionExport,
            onPressed: onExport,
            height: 40,
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
  const PreviewArea({super.key});

  @override
  ConsumerState<PreviewArea> createState() => _PreviewAreaState();
}

class _PreviewAreaState extends ConsumerState<PreviewArea> {
  int? _textureId;
  late final PreviewTransport _transport = ref.read(previewTransportProvider);

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
    return Padding(
      padding: const EdgeInsets.all(ShortedSpace.s),
      child: Center(
        child: AspectRatio(
          aspectRatio: 9 / 16,
          child: ClipRRect(
            borderRadius: BorderRadius.circular(ShortedRadius.m),
            child: ColoredBox(
              color: ShortedColors.bg,
              child: id == null ? null : Texture(textureId: id),
            ),
          ),
        ),
      ),
    );
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
  ConsumerState<TransportBar> createState() => _TransportBarState();
}

class _TransportBarState extends ConsumerState<TransportBar>
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

  void _toggle() {
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
    return SizedBox(
      height: ShortedSize.transport,
      child: Row(
        children: [
          ShortedIconButton(
            icon: _transport.isPlaying ? Icons.pause : Icons.play_arrow,
            tooltip: l10n.playPause,
            onPressed: widget.durationUs > 0 ? _toggle : null,
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
    required this.height,
    required this.view,
    required this.playheadUs,
  });

  final double height;
  final ProjectView view;
  final ValueNotifier<int> playheadUs;

  @override
  ConsumerState<TimelineArea> createState() => _TimelineAreaState();
}

class _TimelineAreaState extends ConsumerState<TimelineArea> {
  /// The selected clip, for the tools. Other tracks get their tools with their tickets.
  TimelineItem? _clip;

  late final _filmstrips = Filmstrips(
    ref.read(projectGatewayProvider).filmstrip,
  );

  @override
  void dispose() {
    _filmstrips.dispose();
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
          items: [
            for (final item in track.items)
              TimelineItem(
                id: item.id,
                startUs: item.startUs.toInt(),
                endUs: item.endUs.toInt(),
                mediaId: item.mediaId,
                sourceInUs: item.sourceInUs.toInt(),
                speed: item.speed,
              ),
          ],
        ),
    ];
    for (final track in widget.view.tracks) {
      for (final item in track.items) {
        final mediaId = item.mediaId;
        if (mediaId != null) _filmstrips.request(mediaId);
      }
    }
    final clip = _clip;
    return SizedBox(
      height: widget.height,
      child: Column(
        children: [
          if (clip != null)
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
                ShortedIconButton(
                  icon: Icons.delete_outline,
                  tooltip: l10n.delete,
                  onPressed: () {
                    setState(() => _clip = null);
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
              onSelect: (item) => setState(() {
                _clip = item != null && _isClip(item) ? item : null;
              }),
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
    required this.media,
    required this.durationUs,
  });

  final EditorSection section;
  final String projectId;
  final List<MediaItemView> media;
  final int durationUs;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return AnimatedSwitcher(
      duration: ShortedMotion.section,
      child: switch (section) {
        EditorSection.clips => ClipsPanel(
          key: const ValueKey(EditorSection.clips),
          projectId: projectId,
          media: media,
        ),
        EditorSection.export => ExportPanel(
          key: const ValueKey(EditorSection.export),
          durationUs: durationUs,
        ),
        _ => Center(
          key: ValueKey(section),
          child: Padding(
            padding: const EdgeInsets.all(ShortedSpace.l),
            child: Text(
              l10n.toolsPlaceholder(section.label(l10n)),
              textAlign: TextAlign.center,
              style: const TextStyle(color: ShortedColors.textMuted),
            ),
          ),
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
