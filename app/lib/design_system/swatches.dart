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
  });

  final String selected;
  final ValueChanged<String> onTap;
  final List<String> colours;

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
    ],
  );
}
