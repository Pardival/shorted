import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/look.dart';
import '../../bridge/api/overlays.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import 'brand_kit_section.dart';
import 'library_section.dart';
import 'text_section.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

const _speeds = [0.5, 1.0, 1.5, 2.0];

String _transitionName(AppLocalizations l10n, TransitionKindView kind) =>
    switch (kind) {
      TransitionKindView.cut => l10n.transitionCut,
      TransitionKindView.crossfade => l10n.transitionCrossfade,
      TransitionKindView.dipToBlack => l10n.transitionDipToBlack,
      TransitionKindView.dipToWhite => l10n.transitionDipToWhite,
      TransitionKindView.flash => l10n.transitionFlash,
      TransitionKindView.slide => l10n.transitionSlide,
      TransitionKindView.push => l10n.transitionPush,
      TransitionKindView.whipPan => l10n.transitionWhipPan,
      TransitionKindView.zoomIn => l10n.transitionZoomIn,
      TransitionKindView.zoomOut => l10n.transitionZoomOut,
    };

class EnhancePanel extends ConsumerWidget {
  const EnhancePanel({super.key, required this.view});

  final ProjectView view;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final editor = ref.read(editorProvider(view.id).notifier);

    Future<void> run(
      Future<ProjectView> Function(ProjectGateway gateway) action,
    ) async {
      try {
        await editor.apply(action);
      } on Object catch (error) {
        if (context.mounted) showToast(context, errorMessage(l10n, error));
      }
    }

    final selected = ref.watch(selectedClipProvider);
    ClipLookView? clip;
    for (final look in view.look.clips) {
      if (look.clipId == selected) clip = look;
    }
    final selectedId = ref.watch(selectedOverlayProvider);
    OverlayView? selectedOverlay;
    for (final overlay in view.overlays) {
      if (overlay.id == selectedId) selectedOverlay = overlay;
    }

    Future<void> addShape(ShapeKindView shape) async {
      final atUs = ref.read(previewTransportProvider).positionUs;
      await run((gateway) => gateway.addShape(shape, atUs));
      if (!context.mounted) return;
      final added = ref.read(editorProvider(view.id)).value?.view.overlays;
      if (added != null && added.isNotEmpty) {
        ref.read(selectedOverlayProvider.notifier).set(added.last.id);
      }
    }

    Widget page(List<Widget> children) => ListView(
      padding: const EdgeInsets.all(ShortedSpace.m),
      children: children,
    );

    final library = page([
      LibrarySection(
        onPlace: (name) =>
            placeLibraryItem(ref, context, view.id, LibraryDrag(name)),
      ),
      const SizedBox(height: ShortedSpace.m),
      Text(l10n.shapes, style: const TextStyle(fontWeight: FontWeight.w600)),
      const SizedBox(height: ShortedSpace.s),
      Wrap(
        spacing: ShortedSpace.s,
        children: [
          for (final (shape, label) in [
            (ShapeKindView.rectangle, l10n.shapeRectangle),
            (ShapeKindView.ellipse, l10n.shapeEllipse),
            (ShapeKindView.arrow, l10n.shapeArrow),
          ])
            ShortedChip(label: label, onTap: () => addShape(shape)),
        ],
      ),
    ]);
    final myAssets = page([
      BrandKitSection(
        onPlace: (id) => placeLibraryItem(
          ref,
          context,
          view.id,
          LibraryDrag(id, fromBrandKit: true),
        ),
      ),
    ]);
    final textPage = page([TextSection(view: view, run: run)]);
    final clipPage = page([
      if (clip == null)
        Text(
          l10n.selectClipForLook,
          style: const TextStyle(color: ShortedColors.textMuted),
        )
      else
        _ClipLook(key: ValueKey(clip.clipId), clip: clip, run: run),
    ]);
    final transitions = page([
      Text(
        l10n.transitions,
        style: const TextStyle(color: ShortedColors.textMuted),
      ),
      const SizedBox(height: ShortedSpace.s),
      Wrap(
        spacing: ShortedSpace.s,
        runSpacing: ShortedSpace.s,
        children: [
          for (final kind in TransitionKindView.values)
            ShortedChip(
              label: _transitionName(l10n, kind),
              selected: view.look.transition == kind,
              onTap: () => run((gateway) => gateway.setTransitionForAll(kind)),
            ),
        ],
      ),
    ]);

    // S40: tabs, with the selected element's own settings above them.
    return DefaultTabController(
      length: 5,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (selectedOverlay != null)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.m),
              child: _OverlayLook(
                key: ValueKey(selectedOverlay.id),
                overlay: selectedOverlay,
                run: run,
              ),
            ),
          TabBar(
            isScrollable: true,
            tabAlignment: TabAlignment.start,
            tabs: [
              Tab(text: l10n.library),
              Tab(text: l10n.myAssets),
              Tab(text: l10n.textTab),
              Tab(text: l10n.clipTab),
              Tab(text: l10n.transitionsTab),
            ],
          ),
          Expanded(
            child: TabBarView(
              children: [library, myAssets, textPage, clipPage, transitions],
            ),
          ),
        ],
      ),
    );
  }
}

/// Opacity and flip of the selected element. The slider is recorded once, when it is let go.
class _OverlayLook extends ConsumerStatefulWidget {
  const _OverlayLook({super.key, required this.overlay, required this.run});

  final OverlayView overlay;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  ConsumerState<_OverlayLook> createState() => _OverlayLookState();
}

class _OverlayLookState extends ConsumerState<_OverlayLook> {
  double? _opacity;

  void _set({double? opacity, bool? flip}) {
    final base = widget.overlay.transform;
    final next = TransformView(
      x: base.x,
      y: base.y,
      scale: base.scale,
      rotation: base.rotation,
      opacity: opacity ?? base.opacity,
      flip: flip ?? base.flip,
    );
    final id = widget.overlay.id;
    // Opacity is keyable; a flip is not, so it always changes the element itself.
    if (ref.read(keyframeModeProvider) && opacity != null) {
      final atUs = ref.read(previewTransportProvider).positionUs;
      final here = ref
          .read(projectGatewayProvider)
          .overlayTransformAt(id, atUs);
      final keyed = TransformView(
        x: (here ?? base).x,
        y: (here ?? base).y,
        scale: (here ?? base).scale,
        rotation: (here ?? base).rotation,
        opacity: opacity,
        flip: base.flip,
      );
      widget.run((gateway) => gateway.keyOverlayTransform(id, atUs, keyed));
      return;
    }
    widget.run((gateway) => gateway.setOverlayTransform(id, next));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final base = widget.overlay.transform;
    return Row(
      children: [
        SizedBox(
          width: 96,
          child: Text(
            l10n.opacity,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        ),
        Expanded(
          child: Slider(
            value: _opacity ?? base.opacity,
            label: l10n.opacity,
            onChanged: widget.overlay.locked
                ? null
                : (value) => setState(() => _opacity = value),
            onChangeEnd: (value) => _set(opacity: value),
          ),
        ),
        ShortedIconButton(
          icon: Icons.flip,
          tooltip: l10n.flip,
          onPressed: widget.overlay.locked
              ? null
              : () => _set(flip: !base.flip),
        ),
      ],
    );
  }
}

/// Sliders follow the finger locally; the engine hears once, when the finger lifts.
class _ClipLook extends StatefulWidget {
  const _ClipLook({super.key, required this.clip, required this.run});

  final ClipLookView clip;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  State<_ClipLook> createState() => _ClipLookState();
}

class _ClipLookState extends State<_ClipLook> {
  double? _brightness;
  double? _contrast;
  double? _saturation;

  void _commit() {
    final clip = widget.clip;
    widget.run(
      (gateway) => gateway.setClipColor(
        clip.clipId,
        brightness: _brightness ?? clip.brightness,
        contrast: _contrast ?? clip.contrast,
        saturation: _saturation ?? clip.saturation,
      ),
    );
  }

  Widget _slider(
    String label,
    double value,
    double min,
    double max,
    ValueChanged<double> onChanged,
  ) => Row(
    children: [
      SizedBox(
        width: 96,
        child: Text(
          label,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: ShortedColors.textMuted),
        ),
      ),
      Expanded(
        child: Slider(
          value: value.clamp(min, max),
          min: min,
          max: max,
          label: label,
          onChanged: onChanged,
          onChangeEnd: (_) => _commit(),
        ),
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final clip = widget.clip;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(l10n.speed, style: const TextStyle(fontWeight: FontWeight.w600)),
        const SizedBox(height: ShortedSpace.s),
        Wrap(
          spacing: ShortedSpace.s,
          children: [
            for (final speed in _speeds)
              ShortedChip(
                label: '$speed×',
                selected: clip.speed == speed,
                onTap: () => widget.run(
                  (gateway) => gateway.setClipSpeed(clip.clipId, speed),
                ),
              ),
          ],
        ),
        const SizedBox(height: ShortedSpace.m),
        Text(l10n.colour, style: const TextStyle(fontWeight: FontWeight.w600)),
        _slider(
          l10n.brightness,
          _brightness ?? clip.brightness,
          -0.5,
          0.5,
          (value) => setState(() => _brightness = value),
        ),
        _slider(
          l10n.contrast,
          _contrast ?? clip.contrast,
          0.5,
          1.5,
          (value) => setState(() => _contrast = value),
        ),
        _slider(
          l10n.saturation,
          _saturation ?? clip.saturation,
          0,
          2,
          (value) => setState(() => _saturation = value),
        ),
      ],
    );
  }
}
