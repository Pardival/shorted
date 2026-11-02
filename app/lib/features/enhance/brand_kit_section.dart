import 'dart:ui' as ui;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/brand_kit.dart';
import '../../bridge/error.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import 'library_section.dart';

const _thumbnailPixels = 128;

const _assetTypes = XTypeGroup(
  label: 'assets',
  extensions: [
    'png',
    'jpg',
    'jpeg',
    'webp',
    'svg',
    'gif',
    'apng',
    'mov',
    'mp4',
    'm4v',
  ],
);

/// My assets, the Brand kit (S40): files kept for every project, placed like library items.
class BrandKitSection extends ConsumerStatefulWidget {
  const BrandKitSection({super.key, required this.onPlace});

  final ValueChanged<String> onPlace;

  @override
  ConsumerState<BrandKitSection> createState() => _BrandKitSectionState();
}

class _BrandKitSectionState extends ConsumerState<BrandKitSection> {
  List<BrandAssetView>? _assets;
  bool _managing = false;
  final _pictures = <String, ui.Image>{};

  @override
  void initState() {
    super.initState();
    _reload();
  }

  @override
  void dispose() {
    for (final picture in _pictures.values) {
      picture.dispose();
    }
    super.dispose();
  }

  Future<void> _reload() async {
    final assets = await ref.read(projectGatewayProvider).brandKit();
    if (!mounted) return;
    setState(() => _assets = assets);
    for (final asset in assets) {
      _load(asset.id);
    }
  }

  void _load(String id) {
    if (_pictures.containsKey(id)) return;
    final thumbnail = ref
        .read(projectGatewayProvider)
        .brandThumbnail(id, _thumbnailPixels);
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
        setState(() => _pictures[id] = image);
      },
    );
  }

  Future<void> _add() async {
    final l10n = AppLocalizations.of(context);
    final files = await openFiles(acceptedTypeGroups: const [_assetTypes]);
    if (files.isEmpty) return;
    try {
      final outcomes = await ref.read(projectGatewayProvider).importBrandAssets(
        [for (final file in files) file.path],
      );
      if (!mounted) return;
      final refused = [for (final outcome in outcomes) ?outcome.error];
      if (refused.contains(ErrorCode.tooLarge)) {
        showToast(context, l10n.errorTooLarge);
      } else if (refused.isNotEmpty) {
        showToast(context, l10n.importFailed(refused.length));
      }
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
    await _reload();
  }

  Future<void> _remove(String id) async {
    await ref.read(projectGatewayProvider).deleteBrandAsset(id);
    _pictures.remove(id)?.dispose();
    await _reload();
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final assets = _assets;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: SecondaryButton(
                label: l10n.addToBrandKit,
                onPressed: _add,
              ),
            ),
            if (assets != null && assets.isNotEmpty)
              TextButton(
                onPressed: () => setState(() => _managing = !_managing),
                child: Text(_managing ? l10n.doneManaging : l10n.manageAssets),
              ),
          ],
        ),
        const SizedBox(height: ShortedSpace.s),
        if (assets != null && assets.isEmpty)
          Text(
            l10n.brandKitEmpty,
            style: const TextStyle(color: ShortedColors.textMuted),
          )
        else if (assets != null)
          Wrap(
            spacing: ShortedSpace.s,
            runSpacing: ShortedSpace.s,
            children: [
              for (final asset in assets)
                libraryTile(
                  drag: LibraryDrag(asset.id, fromBrandKit: true),
                  picture: _pictures[asset.id],
                  onTap: _managing
                      ? () => _remove(asset.id)
                      : () => widget.onPlace(asset.id),
                  badge: _managing
                      ? Positioned(
                          right: -6,
                          top: -6,
                          child: Tooltip(
                            message: l10n.removeAsset,
                            child: const Icon(
                              Icons.remove_circle,
                              color: ShortedColors.danger,
                              size: 20,
                            ),
                          ),
                        )
                      : switch (asset.kind) {
                          BrandKindView.video => const Positioned(
                            right: 2,
                            bottom: 2,
                            child: Icon(Icons.movie_outlined, size: 14),
                          ),
                          BrandKindView.animation => const Positioned(
                            right: 2,
                            bottom: 2,
                            child: Icon(Icons.gif_box_outlined, size: 14),
                          ),
                          BrandKindView.picture => null,
                        },
                ),
            ],
          ),
      ],
    );
  }
}
