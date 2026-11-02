import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/overlays.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/swatches.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

const _fonts = ['Space Grotesk', 'Inter'];
const minTextSize = 24.0;
const maxTextSize = 300.0;

String decorationName(AppLocalizations l10n, TextDecorationView decoration) =>
    switch (decoration) {
      TextDecorationView.plain => l10n.textPlain,
      TextDecorationView.chip => l10n.textChip,
      TextDecorationView.outline => l10n.textOutline,
      TextDecorationView.shadow => l10n.textShadow,
    };

TextElementView copyText(
  TextElementView text, {
  String? content,
  String? font,
  double? size,
  String? color,
  bool? uppercase,
  TextDecorationView? decoration,
}) => TextElementView(
  content: content ?? text.content,
  font: font ?? text.font,
  size: size ?? text.size,
  color: color ?? text.color,
  uppercase: uppercase ?? text.uppercase,
  decoration: decoration ?? text.decoration,
);

/// The Text tab (S40): add a text element, then edit the selected one's words and look.
class TextSection extends ConsumerWidget {
  const TextSection({super.key, required this.view, required this.run});

  final ProjectView view;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final selected = ref.watch(selectedOverlayProvider);
    OverlayView? element;
    for (final overlay in view.overlays) {
      if (overlay.id == selected && overlay.text != null) element = overlay;
    }

    Future<void> add() async {
      final atUs = ref.read(previewTransportProvider).positionUs;
      await run((gateway) => gateway.addText(l10n.textPlaceholder, atUs));
      if (!context.mounted) return;
      final overlays = ref.read(editorProvider(view.id)).value?.view.overlays;
      if (overlays != null && overlays.isNotEmpty) {
        ref.read(selectedOverlayProvider.notifier).set(overlays.last.id);
      }
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SecondaryButton(label: l10n.addText, onPressed: add),
        const SizedBox(height: ShortedSpace.m),
        if (element == null)
          Text(
            l10n.selectTextToEdit,
            style: const TextStyle(color: ShortedColors.textMuted),
          )
        else
          _TextEditor(key: ValueKey(element.id), element: element, run: run),
      ],
    );
  }
}

/// The words follow the keyboard locally and reach the engine when the field is left; every
/// other change is one edit.
class _TextEditor extends StatefulWidget {
  const _TextEditor({super.key, required this.element, required this.run});

  final OverlayView element;
  final Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run;

  @override
  State<_TextEditor> createState() => _TextEditorState();
}

class _TextEditorState extends State<_TextEditor> {
  late final _words = TextEditingController(text: _text.content);
  final _focus = FocusNode();
  double? _size;

  TextElementView get _text => widget.element.text!;

  @override
  void initState() {
    super.initState();
    _focus.addListener(() {
      if (!_focus.hasFocus) _commitWords();
    });
  }

  @override
  void dispose() {
    _words.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _set(TextElementView next) =>
      widget.run((gateway) => gateway.setText(widget.element.id, next));

  void _commitWords() {
    final words = _words.text;
    if (words.trim().isEmpty || words == _text.content) return;
    _set(copyText(_text, content: words));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final text = _text;
    final locked = widget.element.locked;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TextField(
          key: const ValueKey('text-words'),
          controller: _words,
          focusNode: _focus,
          enabled: !locked,
          minLines: 1,
          maxLines: 4,
          keyboardType: TextInputType.multiline,
          decoration: InputDecoration(isDense: true, labelText: l10n.textWords),
          onTapOutside: (_) => _focus.unfocus(),
        ),
        const SizedBox(height: ShortedSpace.s),
        Wrap(
          spacing: ShortedSpace.xs,
          runSpacing: ShortedSpace.xs,
          children: [
            for (final decoration in TextDecorationView.values)
              ShortedChip(
                label: decorationName(l10n, decoration),
                selected: text.decoration == decoration,
                onTap: locked
                    ? null
                    : () => _set(copyText(text, decoration: decoration)),
              ),
          ],
        ),
        const SizedBox(height: ShortedSpace.s),
        Wrap(
          spacing: ShortedSpace.xs,
          children: [
            for (final font in _fonts)
              ShortedChip(
                label: font,
                selected: text.font == font,
                onTap: locked ? null : () => _set(copyText(text, font: font)),
              ),
          ],
        ),
        Row(
          children: [
            SizedBox(
              width: 96,
              child: Text(
                l10n.textSize,
                style: const TextStyle(color: ShortedColors.textMuted),
              ),
            ),
            Expanded(
              child: Slider(
                value: (_size ?? text.size).clamp(minTextSize, maxTextSize),
                min: minTextSize,
                max: maxTextSize,
                label: l10n.textSize,
                onChanged: locked
                    ? null
                    : (value) => setState(() => _size = value),
                onChangeEnd: (value) => _set(copyText(text, size: value)),
              ),
            ),
          ],
        ),
        ColourSwatches(
          selected: text.color,
          onTap: (color) {
            if (!locked) _set(copyText(text, color: color));
          },
        ),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: Text(l10n.captionUppercase),
          value: text.uppercase,
          onChanged: locked
              ? null
              : (value) => _set(copyText(text, uppercase: value)),
        ),
      ],
    );
  }
}
