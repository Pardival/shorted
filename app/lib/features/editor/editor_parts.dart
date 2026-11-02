import 'package:flutter/material.dart';

import '../../bridge/api/media.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../clips/clips_panel.dart';
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

/// Stands in for the engine texture until T-0404. Nothing here may render video content.
class PreviewArea extends StatelessWidget {
  const PreviewArea({super.key});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(ShortedSpace.s),
      child: Center(
        child: AspectRatio(
          aspectRatio: 9 / 16,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: ShortedColors.mediaPlaceholder,
              borderRadius: BorderRadius.circular(ShortedRadius.m),
            ),
            child: Center(
              child: Text(AppLocalizations.of(context).previewPlaceholder),
            ),
          ),
        ),
      ),
    );
  }
}

class TransportBar extends StatelessWidget {
  const TransportBar({
    super.key,
    required this.durationUs,
    required this.onUndo,
    required this.onRedo,
  });

  final int durationUs;
  final VoidCallback? onUndo;
  final VoidCallback? onRedo;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return SizedBox(
      height: ShortedSize.transport,
      child: Row(
        children: [
          ShortedIconButton(
            icon: Icons.play_arrow,
            tooltip: l10n.playPause,
            onPressed: null,
          ),
          const SizedBox(width: ShortedSpace.s),
          Expanded(
            child: Text(
              '0:00 / ${formatTimecode(durationUs)}',
              overflow: TextOverflow.ellipsis,
            ),
          ),
          ShortedIconButton(
            icon: Icons.undo,
            tooltip: l10n.undo,
            onPressed: onUndo,
          ),
          ShortedIconButton(
            icon: Icons.redo,
            tooltip: l10n.redo,
            onPressed: onRedo,
          ),
        ],
      ),
    );
  }
}

class TimelineArea extends StatefulWidget {
  const TimelineArea({super.key, required this.height, required this.view});

  final double height;
  final ProjectView view;

  @override
  State<TimelineArea> createState() => _TimelineAreaState();
}

class _TimelineAreaState extends State<TimelineArea> {
  final _playheadUs = ValueNotifier<int>(0);

  @override
  void dispose() {
    _playheadUs.dispose();
    super.dispose();
  }

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
              ),
          ],
        ),
    ];
    return SizedBox(
      height: widget.height,
      child: TimelineView(
        tracks: tracks,
        durationUs: widget.view.durationUs.toInt(),
        playheadUs: _playheadUs,
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
  });

  final EditorSection section;
  final String projectId;
  final List<MediaItemView> media;

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
