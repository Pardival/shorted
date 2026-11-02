import 'package:desktop_drop/desktop_drop.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/media.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
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
              SecondaryButton(label: l10n.addMedia, onPressed: _pickFiles),
              if (isDesktop && media.isNotEmpty) ...[
                const SizedBox(height: ShortedSpace.s),
                SecondaryButton(
                  label: l10n.collectMedia,
                  onPressed: () => _guard(() async {
                    await _editor.apply((gateway) => gateway.collectMedia());
                    if (context.mounted) {
                      showToast(context, l10n.mediaCollected);
                    }
                  }),
                ),
              ],
              const SizedBox(height: ShortedSpace.s),
              Expanded(
                child: media.isEmpty
                    ? Center(
                        child: Text(
                          l10n.dropMedia,
                          style: const TextStyle(
                            color: ShortedColors.textMuted,
                          ),
                        ),
                      )
                    : ListView.builder(
                        itemCount: media.length,
                        itemBuilder: (context, index) {
                          final item = media[index];
                          final order = _picked.indexOf(item.id);
                          return _MediaRow(
                            item: item,
                            order: order < 0 ? null : order + 1,
                            onTap: item.offline ? null : () => _toggle(item),
                            onRelink: () => _relink(item),
                          );
                        },
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
