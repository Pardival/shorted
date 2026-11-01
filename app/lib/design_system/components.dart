import 'package:flutter/material.dart';

import 'tokens.g.dart';

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
    return FilledButton.icon(
      onPressed: onPressed,
      icon: icon == null ? null : Icon(icon, size: 20),
      label: Text(label),
      style: FilledButton.styleFrom(
        minimumSize: Size(0, height),
        backgroundColor: ShortedColors.accent,
        foregroundColor: ShortedColors.onAccent,
        textStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15),
        padding: const EdgeInsets.symmetric(horizontal: ShortedSpace.xl),
      ),
    );
  }
}

class SecondaryButton extends StatelessWidget {
  const SecondaryButton({
    super.key,
    required this.label,
    required this.onPressed,
  });

  final String label;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    return OutlinedButton(
      onPressed: onPressed,
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(0, ShortedSize.touchTarget),
        foregroundColor: ShortedColors.text,
        side: const BorderSide(color: ShortedColors.lineStrong),
      ),
      child: Text(label),
    );
  }
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
  const ShortedChip({super.key, required this.label, this.selected = false});

  final String label;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    return Container(
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
