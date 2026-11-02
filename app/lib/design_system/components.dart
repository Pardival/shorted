import 'package:flutter/material.dart';

import 'tokens.g.dart';

/// A label on one line, cut short rather than wrapped: buttons keep their shape at any width
/// and in every language (T-1210).
Widget _oneLine(String label) =>
    Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, softWrap: false);

final _buttonShape = RoundedRectangleBorder(
  borderRadius: BorderRadius.circular(ShortedRadius.m),
);

class PrimaryButton extends StatelessWidget {
  const PrimaryButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
    this.height = ShortedSize.primaryButton,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final double height;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: label,
      excludeFromSemantics: true,
      waitDuration: const Duration(seconds: 1),
      child: FilledButton.icon(
        onPressed: onPressed,
        icon: icon == null ? null : Icon(icon, size: 20),
        label: _oneLine(label),
        style: FilledButton.styleFrom(
          minimumSize: Size(0, height),
          shape: _buttonShape,
          backgroundColor: ShortedColors.accent,
          foregroundColor: ShortedColors.onAccent,
          textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
          padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.l),
        ),
      ),
    );
  }
}

/// The everyday action: a quiet filled button, with an optional icon.
class SecondaryButton extends StatelessWidget {
  const SecondaryButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.icon,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final style = OutlinedButton.styleFrom(
      minimumSize: const Size(0, ShortedSize.touchTarget),
      shape: _buttonShape,
      foregroundColor: ShortedColors.text,
      backgroundColor: ShortedColors.surfaceRaised,
      side: const BorderSide(color: ShortedColors.line),
      padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.m),
      textStyle: const TextStyle(fontWeight: FontWeight.w500, fontSize: 14),
    );
    final button = icon == null
        ? OutlinedButton(
            onPressed: onPressed,
            style: style,
            child: _oneLine(label),
          )
        : OutlinedButton.icon(
            onPressed: onPressed,
            style: style,
            icon: Icon(icon, size: 18),
            label: _oneLine(label),
          );
    return Tooltip(
      message: label,
      excludeFromSemantics: true,
      waitDuration: const Duration(seconds: 1),
      child: button,
    );
  }
}

/// A row of tools, centred, that scrolls sideways when the screen is too narrow for it.
class ToolStrip extends StatelessWidget {
  const ToolStrip({super.key, required this.children});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) => Center(
    child: SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: Row(mainAxisSize: MainAxisSize.min, children: children),
    ),
  );
}

/// Icon action with the 44 pt minimum target and a required screen reader label.
class ShortedIconButton extends StatelessWidget {
  const ShortedIconButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return IconButton(
      icon: Icon(icon, size: 22),
      tooltip: tooltip,
      onPressed: onPressed,
      color: ShortedColors.text,
      constraints: const BoxConstraints.tightFor(
        width: ShortedSize.touchTarget,
        height: ShortedSize.touchTarget,
      ),
    );
  }
}

class ShortedChip extends StatelessWidget {
  const ShortedChip({
    super.key,
    required this.label,
    this.selected = false,
    this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final chip = Container(
      padding: const EdgeInsets.symmetric(
        horizontal: ShortedSpace.m,
        vertical: ShortedSpace.xs,
      ),
      decoration: BoxDecoration(
        color: selected ? ShortedColors.accent : ShortedColors.surfaceRaised,
        borderRadius: BorderRadius.circular(ShortedRadius.pill),
      ),
      child: Text(
        label,
        style: Theme.of(context).textTheme.labelMedium?.copyWith(
          color: selected ? ShortedColors.onAccent : ShortedColors.text,
        ),
      ),
    );
    if (onTap == null) return chip;
    return Semantics(
      button: true,
      selected: selected,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(ShortedRadius.pill),
        child: chip,
      ),
    );
  }
}

class ShortedListRow extends StatelessWidget {
  const ShortedListRow({
    super.key,
    required this.title,
    this.subtitle,
    this.onTap,
  });

  final String title;
  final String? subtitle;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final detail = subtitle;
    return ListTile(
      title: Text(title),
      subtitle: detail == null
          ? null
          : Text(
              detail,
              style: const TextStyle(color: ShortedColors.textMuted),
            ),
      onTap: onTap,
    );
  }
}

class ProgressRing extends StatelessWidget {
  const ProgressRing({super.key, required this.value, this.size = 44});

  /// 0 to 1, or null when the duration is unknown.
  final double? value;
  final double size;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CircularProgressIndicator(
        value: value,
        strokeWidth: 3,
        color: ShortedColors.accent,
        backgroundColor: ShortedColors.line,
      ),
    );
  }
}

void showToast(BuildContext context, String message) {
  ScaffoldMessenger.of(context).showSnackBar(
    SnackBar(
      content: Text(message),
      backgroundColor: ShortedColors.surfaceRaised,
      behavior: SnackBarBehavior.floating,
    ),
  );
}
