import 'package:flutter/material.dart';

import 'tokens.g.dart';

/// The colours offered for words on the video: captions and text elements share them.
const textColours = [
  '#FFFFFFFF',
  '#111114FF',
  '#FFE14DFF',
  '#C6F135FF',
  '#FF5C7AFF',
  '#4DD2FFFF',
  '#9A8CFFFF',
];

/// `#RRGGBBAA` as an opaque colour.
Color colourOf(String hex) =>
    Color(int.parse('FF${hex.substring(1, 7)}', radix: 16));

class ColourSwatches extends StatelessWidget {
  const ColourSwatches({
    super.key,
    required this.selected,
    required this.onTap,
    this.colours = textColours,
    this.customLabel,
  });

  final String selected;
  final ValueChanged<String> onTap;
  final List<String> colours;

  /// When set, a last swatch opens a picker for any colour, with this label.
  final String? customLabel;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: ShortedSpace.s,
    runSpacing: ShortedSpace.s,
    children: [
      for (final hex in colours)
        Semantics(
          button: true,
          selected: hex == selected,
          label: hex,
          child: InkWell(
            onTap: () => onTap(hex),
            child: Container(
              width: 28,
              height: 28,
              decoration: BoxDecoration(
                color: colourOf(hex),
                shape: BoxShape.circle,
                border: Border.all(
                  color: hex == selected
                      ? ShortedColors.text
                      : ShortedColors.line,
                  width: 2,
                ),
              ),
            ),
          ),
        ),
      if (customLabel case final label?)
        Tooltip(
          message: label,
          child: Semantics(
            button: true,
            label: label,
            child: InkWell(
              onTap: () async {
                final picked = await pickColour(context, selected, label);
                if (picked != null) onTap(picked);
              },
              child: Container(
                width: 28,
                height: 28,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(color: ShortedColors.line, width: 2),
                  gradient: const SweepGradient(
                    colors: [
                      Color(0xFFFF5C7A),
                      Color(0xFFFFE14D),
                      Color(0xFFC6F135),
                      Color(0xFF4DD2FF),
                      Color(0xFF9A8CFF),
                      Color(0xFFFF5C7A),
                    ],
                  ),
                ),
                child: const Icon(Icons.add, size: 16, color: Colors.black87),
              ),
            ),
          ),
        ),
    ],
  );
}

/// `#RRGGBB` or `#RRGGBBAA` typed by the user, as `#RRGGBBAA`, or null.
String? parseHexColour(String text) {
  final digits = text.trim().replaceFirst('#', '').toUpperCase();
  if (!RegExp(r'^[0-9A-F]{6}([0-9A-F]{2})?$').hasMatch(digits)) return null;
  return '#${digits.length == 6 ? '${digits}FF' : digits}';
}

/// Any colour: a hue to slide along, and its code to type.
Future<String?> pickColour(
  BuildContext context,
  String initial,
  String title,
) => showDialog<String>(
  context: context,
  builder: (context) => _ColourDialog(initial: initial, title: title),
);

class _ColourDialog extends StatefulWidget {
  const _ColourDialog({required this.initial, required this.title});

  final String initial;
  final String title;

  @override
  State<_ColourDialog> createState() => _ColourDialogState();
}

class _ColourDialogState extends State<_ColourDialog> {
  late String _hex = parseHexColour(widget.initial) ?? '#FF5C7AFF';
  late final _code = TextEditingController(text: _hex.substring(0, 7));

  @override
  void dispose() {
    _code.dispose();
    super.dispose();
  }

  void _setHue(double hue) {
    final colour = HSVColor.fromAHSV(1, hue, 0.75, 1).toColor();
    final rgb = colour.toARGB32() & 0xFFFFFF;
    final hex = '#${rgb.toRadixString(16).padLeft(6, '0').toUpperCase()}FF';
    setState(() => _hex = hex);
    _code.text = hex.substring(0, 7);
  }

  @override
  Widget build(BuildContext context) {
    final hue = HSVColor.fromColor(colourOf(_hex)).hue;
    return AlertDialog(
      title: Text(widget.title),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            height: 48,
            decoration: BoxDecoration(
              color: colourOf(_hex),
              borderRadius: BorderRadius.circular(ShortedRadius.s),
            ),
          ),
          Slider(value: hue, max: 360, onChanged: _setHue),
          TextField(
            key: const ValueKey('colour-code'),
            controller: _code,
            decoration: const InputDecoration(prefixText: ''),
            onChanged: (text) {
              final parsed = parseHexColour(text);
              if (parsed != null) setState(() => _hex = parsed);
            },
          ),
        ],
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(MaterialLocalizations.of(context).cancelButtonLabel),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, _hex),
          child: Text(MaterialLocalizations.of(context).okButtonLabel),
        ),
      ],
    );
  }
}
