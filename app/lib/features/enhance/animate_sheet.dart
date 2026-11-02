import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import 'overlay_selection.dart' show canvasWidth;

const _thumbSize = 56.0;
const _elementSize = 22.0;
const _thumbCycle = Duration(milliseconds: 1400);

/// The presets each slot offers (FR-ENH-20).
const presetsFor = {
  AnimSlotView.in_: [
    PresetKindView.pop,
    PresetKindView.slideLeft,
    PresetKindView.slideRight,
    PresetKindView.slideUp,
    PresetKindView.slideDown,
    PresetKindView.fade,
    PresetKindView.zoom,
    PresetKindView.stamp,
    PresetKindView.wipe,
  ],
  AnimSlotView.loop: [
    PresetKindView.float,
    PresetKindView.pulse,
    PresetKindView.wiggle,
    PresetKindView.rotate,
    PresetKindView.bounce,
  ],
  AnimSlotView.out: [
    PresetKindView.fade,
    PresetKindView.shrink,
    PresetKindView.slideLeft,
    PresetKindView.slideRight,
    PresetKindView.slideUp,
    PresetKindView.slideDown,
    PresetKindView.pop,
  ],
};

/// Default and longest durations, in seconds.
const _defaultSeconds = {
  AnimSlotView.in_: 0.5,
  AnimSlotView.loop: 1.2,
  AnimSlotView.out: 0.5,
};
const _maxSeconds = {
  AnimSlotView.in_: 2.0,
  AnimSlotView.loop: 4.0,
  AnimSlotView.out: 2.0,
};
const _minSeconds = 0.1;

String presetName(AppLocalizations l10n, PresetKindView kind) => switch (kind) {
  PresetKindView.pop => l10n.presetPop,
  PresetKindView.slideLeft => l10n.presetSlideLeft,
  PresetKindView.slideRight => l10n.presetSlideRight,
  PresetKindView.slideUp => l10n.presetSlideUp,
  PresetKindView.slideDown => l10n.presetSlideDown,
  PresetKindView.fade => l10n.presetFade,
  PresetKindView.zoom => l10n.presetZoom,
  PresetKindView.stamp => l10n.presetStamp,
  PresetKindView.wipe => l10n.presetWipe,
  PresetKindView.shrink => l10n.presetShrink,
  PresetKindView.float => l10n.presetFloat,
  PresetKindView.pulse => l10n.presetPulse,
  PresetKindView.wiggle => l10n.presetWiggle,
  PresetKindView.rotate => l10n.presetRotate,
  PresetKindView.bounce => l10n.presetBounce,
};

Future<void> showAnimateSheet(
  BuildContext context, {
  required String projectId,
  required String overlayId,
}) => showModalBottomSheet<void>(
  context: context,
  isScrollControlled: true,
  backgroundColor: ShortedColors.surface,
  builder: (context) =>
      AnimateSheet(projectId: projectId, overlayId: overlayId),
);

/// S40 Animate: In, Loop and Out presets as moving thumbnails, each with its duration.
class AnimateSheet extends ConsumerWidget {
  const AnimateSheet({
    super.key,
    required this.projectId,
    required this.overlayId,
  });

  final String projectId;
  final String overlayId;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final view = ref.watch(editorProvider(projectId)).value?.view;
    OverlayView? element;
    for (final overlay in view?.overlays ?? const <OverlayView>[]) {
      if (overlay.id == overlayId) element = overlay;
    }
    if (element == null) return const SizedBox.shrink();

    Future<void> set(
      AnimSlotView slot,
      PresetKindView? kind,
      double seconds,
    ) async {
      try {
        await ref
            .read(editorProvider(projectId).notifier)
            .apply(
              (gateway) => gateway.setPresetAnim(
                overlayId,
                slot,
                kind,
                (seconds * 1e6).round(),
              ),
            );
      } on Object catch (error) {
        if (context.mounted) showToast(context, errorMessage(l10n, error));
      }
    }

    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(ShortedSpace.m),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(l10n.animate, style: Theme.of(context).textTheme.titleMedium),
            for (final (slot, label, current) in [
              (AnimSlotView.in_, l10n.animateIn, element.animIn),
              (AnimSlotView.loop, l10n.animateLoop, element.animLoop),
              (AnimSlotView.out, l10n.animateOut, element.animOut),
            ])
              _SlotRow(
                key: ValueKey('slot-${slot.name}'),
                slot: slot,
                label: label,
                current: current,
                onSet: (kind, seconds) => set(slot, kind, seconds),
              ),
          ],
        ),
      ),
    );
  }
}

class _SlotRow extends StatefulWidget {
  const _SlotRow({
    super.key,
    required this.slot,
    required this.label,
    required this.current,
    required this.onSet,
  });

  final AnimSlotView slot;
  final String label;
  final PresetView? current;
  final void Function(PresetKindView? kind, double seconds) onSet;

  @override
  State<_SlotRow> createState() => _SlotRowState();
}

class _SlotRowState extends State<_SlotRow> {
  /// Follows the slider; the engine hears when it is let go.
  double? _seconds;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final current = widget.current;
    final most = _maxSeconds[widget.slot]!;
    final seconds = current == null
        ? (_seconds ?? _defaultSeconds[widget.slot]!)
        : (_seconds ?? current.durationUs.toInt() / 1e6);
    return Padding(
      padding: const EdgeInsets.only(top: ShortedSpace.m),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            widget.label,
            style: const TextStyle(fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: ShortedSpace.s),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                _Choice(
                  label: l10n.presetNone,
                  selected: current == null,
                  onTap: () => widget.onSet(null, seconds),
                  child: const Icon(
                    Icons.block,
                    color: ShortedColors.textMuted,
                  ),
                ),
                for (final kind in presetsFor[widget.slot]!)
                  _Choice(
                    key: ValueKey('preset-${widget.slot.name}-${kind.name}'),
                    label: presetName(l10n, kind),
                    selected: current?.kind == kind,
                    onTap: () => widget.onSet(kind, seconds),
                    child: PresetThumb(kind: kind, slot: widget.slot),
                  ),
              ],
            ),
          ),
          if (current != null)
            Row(
              children: [
                Expanded(
                  child: Slider(
                    value: seconds.clamp(_minSeconds, most),
                    min: _minSeconds,
                    max: most,
                    label: l10n.seconds(seconds.toStringAsFixed(1)),
                    onChanged: (value) => setState(() => _seconds = value),
                    onChangeEnd: (value) => widget.onSet(current.kind, value),
                  ),
                ),
                Text(l10n.seconds(seconds.toStringAsFixed(1))),
              ],
            ),
        ],
      ),
    );
  }
}

class _Choice extends StatelessWidget {
  const _Choice({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
    required this.child,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;
  final Widget child;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(right: ShortedSpace.s),
    child: Semantics(
      button: true,
      selected: selected,
      label: label,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(ShortedRadius.s),
        child: Column(
          children: [
            Container(
              width: _thumbSize,
              height: _thumbSize,
              decoration: BoxDecoration(
                color: ShortedColors.bg,
                borderRadius: BorderRadius.circular(ShortedRadius.s),
                border: Border.all(
                  color: selected ? ShortedColors.overlay : ShortedColors.line,
                  width: 2,
                ),
              ),
              child: Center(child: child),
            ),
            const SizedBox(height: ShortedSpace.xs),
            SizedBox(
              width: _thumbSize + ShortedSpace.s,
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                textAlign: TextAlign.center,
                style: const TextStyle(fontSize: 11),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

/// A small square moved by the engine's own preset, round and round.
class PresetThumb extends ConsumerStatefulWidget {
  const PresetThumb({super.key, required this.kind, required this.slot});

  final PresetKindView kind;
  final AnimSlotView slot;

  @override
  ConsumerState<PresetThumb> createState() => _PresetThumbState();
}

class _PresetThumbState extends ConsumerState<PresetThumb>
    with SingleTickerProviderStateMixin {
  late final _cycle = AnimationController(vsync: this, duration: _thumbCycle)
    ..repeat();

  @override
  void dispose() {
    _cycle.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final gateway = ref.read(projectGatewayProvider);
    // Offsets are canvas units; the thumbnail stands for the whole frame width.
    const unit = _thumbSize / canvasWidth * 2;
    return ClipRect(
      child: SizedBox.square(
        dimension: _thumbSize,
        child: AnimatedBuilder(
          animation: _cycle,
          builder: (context, _) {
            final sample = gateway.presetSample(
              widget.kind,
              widget.slot,
              _cycle.value,
            );
            return Transform.translate(
              offset: Offset(sample.dx * unit, sample.dy * unit),
              child: Transform.rotate(
                angle: sample.rotation * math.pi / 180,
                child: Transform.scale(
                  scale: sample.scale,
                  child: Opacity(
                    opacity: sample.opacity.clamp(0, 1),
                    child: Center(
                      child: ClipRect(
                        child: Align(
                          alignment: Alignment.centerLeft,
                          widthFactor: sample.reveal.clamp(0, 1),
                          child: Container(
                            width: _elementSize,
                            height: _elementSize,
                            decoration: BoxDecoration(
                              color: ShortedColors.overlay,
                              borderRadius: BorderRadius.circular(4),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}
