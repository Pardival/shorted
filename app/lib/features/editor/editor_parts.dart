import 'package:flutter/material.dart';

import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../timeline/timeline_layout.dart';
import '../timeline/timeline_view.dart';
import 'editor_section.dart';

class EditorTopBar extends StatelessWidget {
  const EditorTopBar({
    super.key,
    required this.onClose,
    required this.onExport,
  });

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
            child: Text(
              l10n.untitledProject,
              overflow: TextOverflow.ellipsis,
              style: text.titleMedium,
            ),
          ),
          const _SavedDot(),
          const SizedBox(width: ShortedSpace.xs),
          Flexible(
            child: Text(
              l10n.saved,
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
  const _SavedDot();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 6,
      height: 6,
      decoration: const BoxDecoration(
        color: ShortedColors.accent,
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
  const TransportBar({super.key});

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
          const Expanded(
            child: Text('0:00 / 0:00', overflow: TextOverflow.ellipsis),
          ),
          ShortedIconButton(
            icon: Icons.undo,
            tooltip: l10n.undo,
            onPressed: null,
          ),
          ShortedIconButton(
            icon: Icons.redo,
            tooltip: l10n.redo,
            onPressed: null,
          ),
        ],
      ),
    );
  }
}

class TimelineArea extends StatefulWidget {
  const TimelineArea({super.key, required this.height});

  final double height;

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
    // Empty until the editor is connected to the engine.
    final tracks = [
      TimelineTrack(
        kind: TrackKind.widgets,
        label: l10n.trackWidgets,
        items: const [],
      ),
      TimelineTrack(
        kind: TrackKind.video,
        label: l10n.trackVideo,
        items: const [],
      ),
      TimelineTrack(
        kind: TrackKind.captions,
        label: l10n.trackCaptions,
        items: const [],
      ),
      TimelineTrack(
        kind: TrackKind.voiceOver,
        label: l10n.trackVoiceOver,
        items: const [],
      ),
    ];
    return SizedBox(
      height: widget.height,
      child: TimelineView(
        tracks: tracks,
        durationUs: 0,
        playheadUs: _playheadUs,
      ),
    );
  }
}

/// Tools of the active section; only this part animates when the section changes.
class SectionPanel extends StatelessWidget {
  const SectionPanel({super.key, required this.section});

  final EditorSection section;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return AnimatedSwitcher(
      duration: ShortedMotion.section,
      child: Center(
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
