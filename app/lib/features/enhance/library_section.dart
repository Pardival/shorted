import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

const _tileSize = 64.0;
const _thumbnailPixels = 128;

/// What is carried while a library item is dragged onto the preview.
class LibraryDrag {
  const LibraryDrag(this.name);

  final String name;
}

const libraryCategories = [
  'badges',
  'arrows',
  'stickers',
  'shapes',
  'doodles',
  'bubbles',
  'highlights',
  'labels',
];

String categoryName(AppLocalizations l10n, String category) =>
    switch (category) {
      'badges' => l10n.categoryBadges,
      'arrows' => l10n.categoryArrows,
      'stickers' => l10n.categoryStickers,
      'shapes' => l10n.shapes,
      'doodles' => l10n.categoryDoodles,
      'bubbles' => l10n.categoryBubbles,
      'highlights' => l10n.categoryHighlights,
      _ => l10n.categoryLabels,
    };

/// Phones lift an item with a long press, so the panel still scrolls; desktops drag at once.
bool get _touchFirst =>
    defaultTargetPlatform == TargetPlatform.iOS ||
    defaultTargetPlatform == TargetPlatform.android;

/// The Shorted library (S40): categories, search, and items to tap or drag onto the preview.
class LibrarySection extends ConsumerStatefulWidget {
  const LibrarySection({super.key, required this.onPlace});

  /// A tap: the item goes in the middle of the frame, from the playhead.
  final ValueChanged<String> onPlace;

  @override
  ConsumerState<LibrarySection> createState() => _LibrarySectionState();
}

class _LibrarySectionState extends ConsumerState<LibrarySection> {
  String? _category;
  String _query = '';
  final _pictures = <String, ui.Image>{};

  @override
  void dispose() {
    for (final picture in _pictures.values) {
      picture.dispose();
    }
    super.dispose();
  }

  void _load(ProjectGateway gateway, String name) {
    if (_pictures.containsKey(name)) return;
    final thumbnail = gateway.libraryThumbnail(name, _thumbnailPixels);
    if (thumbnail == null) return;
    ui.decodeImageFromPixels(
      thumbnail.rgba,
      thumbnail.width,
      thumbnail.height,
      ui.PixelFormat.rgba8888,
      (image) {
        if (!mounted) {
          image.dispose();
          return;
        }
        setState(() => _pictures[name] = image);
      },
    );
  }

  Widget _tile(String name) {
    final picture = _pictures[name];
    final tile = SizedBox.square(
      dimension: _tileSize,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: ShortedColors.surfaceRaised,
          borderRadius: BorderRadius.circular(ShortedRadius.s),
        ),
        child: Padding(
          padding: const EdgeInsets.all(ShortedSpace.xs),
          child: picture == null ? null : RawImage(image: picture),
        ),
      ),
    );
    final feedback = Opacity(opacity: 0.8, child: tile);
    final drag = LibraryDrag(name);
    final draggable = _touchFirst
        ? LongPressDraggable<LibraryDrag>(
            data: drag,
            dragAnchorStrategy: pointerDragAnchorStrategy,
            feedback: feedback,
            child: tile,
          )
        : Draggable<LibraryDrag>(
            data: drag,
            dragAnchorStrategy: pointerDragAnchorStrategy,
            feedback: feedback,
            child: tile,
          );
    return Semantics(
      button: true,
      label: name,
      child: GestureDetector(
        key: ValueKey('library-$name'),
        onTap: () => widget.onPlace(name),
        child: draggable,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final gateway = ref.read(projectGatewayProvider);
    final items = gateway.libraryItems(_query.trim().toLowerCase(), _category);
    for (final item in items) {
      _load(gateway, item.name);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(l10n.library, style: const TextStyle(fontWeight: FontWeight.w600)),
        const SizedBox(height: ShortedSpace.s),
        TextField(
          decoration: InputDecoration(
            isDense: true,
            prefixIcon: const Icon(Icons.search),
            hintText: l10n.searchLibrary,
          ),
          onChanged: (value) => setState(() => _query = value),
        ),
        const SizedBox(height: ShortedSpace.s),
        Wrap(
          spacing: ShortedSpace.xs,
          runSpacing: ShortedSpace.xs,
          children: [
            ShortedChip(
              label: l10n.allCategories,
              selected: _category == null,
              onTap: () => setState(() => _category = null),
            ),
            for (final category in libraryCategories)
              ShortedChip(
                label: categoryName(l10n, category),
                selected: _category == category,
                onTap: () => setState(() => _category = category),
              ),
          ],
        ),
        const SizedBox(height: ShortedSpace.s),
        if (items.isEmpty)
          Text(
            l10n.nothingFound,
            style: const TextStyle(color: ShortedColors.textMuted),
          )
        else
          Wrap(
            spacing: ShortedSpace.s,
            runSpacing: ShortedSpace.s,
            children: [for (final item in items) _tile(item.name)],
          ),
      ],
    );
  }
}

/// Places a library item and selects it.
Future<void> placeLibraryItem(
  WidgetRef ref,
  BuildContext context,
  String projectId,
  String name, {
  double? x,
  double? y,
}) async {
  final l10n = AppLocalizations.of(context);
  final atUs = ref.read(previewTransportProvider).positionUs;
  try {
    await ref
        .read(editorProvider(projectId).notifier)
        .apply((gateway) => gateway.addLibraryItem(name, atUs, x: x, y: y));
    if (!context.mounted) return;
    final overlays = ref.read(editorProvider(projectId)).value?.view.overlays;
    if (overlays != null && overlays.isNotEmpty) {
      ref.read(selectedOverlayProvider.notifier).set(overlays.last.id);
    }
  } on Object catch (error) {
    if (context.mounted) showToast(context, errorMessage(l10n, error));
  }
}
