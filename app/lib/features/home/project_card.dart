import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../../bridge/api/projects.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_providers.dart';
import '../timeline/timeline_layout.dart';

enum ProjectAction { rename, duplicate, delete }

final _coverProvider = FutureProvider.autoDispose.family<ui.Image?, String>((
  ref,
  id,
) async {
  final view = await ref.read(projectGatewayProvider).cover(id);
  if (view == null) return null;
  final done = Completer<ui.Image>();
  ui.decodeImageFromPixels(
    view.rgba,
    view.width,
    view.height,
    ui.PixelFormat.rgba8888,
    done.complete,
  );
  final image = await done.future;
  ref.onDispose(image.dispose);
  return image;
});

class ProjectCard extends ConsumerWidget {
  const ProjectCard({
    super.key,
    required this.project,
    required this.onOpen,
    required this.onAction,
  });

  final ProjectSummary project;
  final VoidCallback onOpen;
  final ValueChanged<ProjectAction> onAction;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final cover = ref.watch(_coverProvider(project.id)).value;
    final locale = Localizations.localeOf(context).toString();
    final edited = DateTime.fromMillisecondsSinceEpoch(
      project.editedAtMs.toInt(),
    );
    final text = Theme.of(context).textTheme;
    return InkWell(
      borderRadius: BorderRadius.circular(ShortedRadius.m),
      onTap: onOpen,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Container(
              clipBehavior: Clip.antiAlias,
              decoration: BoxDecoration(
                color: ShortedColors.mediaPlaceholder,
                borderRadius: BorderRadius.circular(ShortedRadius.m),
                image: cover == null
                    ? null
                    : DecorationImage(
                        image: _CoverImage(cover),
                        fit: BoxFit.cover,
                      ),
              ),
              alignment: Alignment.bottomRight,
              padding: const EdgeInsets.all(ShortedSpace.s),
              child: Text(
                formatTimecode(project.durationUs.toInt()),
                style: text.labelSmall,
              ),
            ),
          ),
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      project.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                    Text(
                      DateFormat.yMMMd(locale).format(edited),
                      style: text.labelSmall?.copyWith(
                        color: ShortedColors.textMuted,
                      ),
                    ),
                  ],
                ),
              ),
              PopupMenuButton<ProjectAction>(
                tooltip: l10n.projectActions,
                onSelected: onAction,
                itemBuilder: (context) => [
                  PopupMenuItem(
                    value: ProjectAction.rename,
                    child: Text(l10n.rename),
                  ),
                  PopupMenuItem(
                    value: ProjectAction.duplicate,
                    child: Text(l10n.duplicate),
                  ),
                  PopupMenuItem(
                    value: ProjectAction.delete,
                    child: Text(l10n.delete),
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Paints an image already in memory.
class _CoverImage extends ImageProvider<_CoverImage> {
  const _CoverImage(this.image);

  final ui.Image image;

  @override
  Future<_CoverImage> obtainKey(ImageConfiguration configuration) =>
      SynchronousFuture(this);

  @override
  ImageStreamCompleter loadImage(
    _CoverImage key,
    ImageDecoderCallback decode,
  ) => OneFrameImageStreamCompleter(
    SynchronousFuture(ImageInfo(image: image.clone())),
  );

  @override
  bool operator ==(Object other) =>
      other is _CoverImage && other.image == image;

  @override
  int get hashCode => image.hashCode;
}
