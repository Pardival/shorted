import 'dart:async';

import 'package:flutter/material.dart';

import 'components.dart';
import 'tokens.g.dart';

/// How long work has to last before it shows, and before it takes over the screen.
const taskInlineAfter = Duration(milliseconds: 300);
const taskOverlayAfter = Duration(seconds: 2);

enum TaskDisplay { nothing, inline, overlay }

TaskDisplay taskDisplayFor(Duration elapsed) {
  if (elapsed < taskInlineAfter) return TaskDisplay.nothing;
  return elapsed < taskOverlayAfter ? TaskDisplay.inline : TaskDisplay.overlay;
}

/// Shows running work by how long it lasts: nothing, then a small indicator, then an overlay
/// with its label, percentage and a cancel button.
class TaskProgress extends StatefulWidget {
  const TaskProgress({
    super.key,
    required this.label,
    required this.fraction,
    required this.cancelLabel,
    required this.onCancel,
  });

  final String label;
  final double? fraction;
  final String cancelLabel;
  final VoidCallback onCancel;

  @override
  State<TaskProgress> createState() => _TaskProgressState();
}

class _TaskProgressState extends State<TaskProgress> {
  final _portal = OverlayPortalController();
  var _display = TaskDisplay.nothing;
  final _timers = <Timer>[];

  @override
  void initState() {
    super.initState();
    _timers
      ..add(Timer(taskInlineAfter, () => _show(TaskDisplay.inline)))
      ..add(Timer(taskOverlayAfter, () => _show(TaskDisplay.overlay)));
  }

  void _show(TaskDisplay display) {
    setState(() => _display = display);
    if (display == TaskDisplay.overlay) _portal.show();
  }

  @override
  void dispose() {
    for (final timer in _timers) {
      timer.cancel();
    }
    super.dispose();
  }

  Widget _details() => Column(
    mainAxisSize: MainAxisSize.min,
    children: [
      ProgressRing(value: widget.fraction),
      const SizedBox(height: ShortedSpace.m),
      Text(widget.label, textAlign: TextAlign.center),
      TextButton(onPressed: widget.onCancel, child: Text(widget.cancelLabel)),
    ],
  );

  @override
  Widget build(BuildContext context) {
    return OverlayPortal(
      controller: _portal,
      overlayChildBuilder: (context) => Positioned.fill(
        child: ColoredBox(
          color: ShortedColors.bg.withValues(alpha: _overlayShade),
          child: Center(
            child: Material(type: MaterialType.transparency, child: _details()),
          ),
        ),
      ),
      child: _display == TaskDisplay.inline
          ? Center(child: _details())
          : const SizedBox.shrink(),
    );
  }
}

const _overlayShade = 0.85;
