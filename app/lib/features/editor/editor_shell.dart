import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import 'editor_parts.dart';
import 'editor_section.dart';

const _phoneTimelineHeight = 132.0;
const _desktopTimelineHeight = 220.0;

/// The section lives in the route, so resizing the window keeps it (T-0202).
class EditorShell extends ConsumerStatefulWidget {
  const EditorShell({
    super.key,
    required this.projectId,
    required this.section,
  });

  final String projectId;
  final EditorSection section;

  @override
  ConsumerState<EditorShell> createState() => _EditorShellState();
}

class _EditorShellState extends ConsumerState<EditorShell> {
  final _playheadUs = ValueNotifier<int>(0);

  String get projectId => widget.projectId;
  EditorSection get section => widget.section;

  @override
  void dispose() {
    _playheadUs.dispose();
    super.dispose();
  }

  void _select(BuildContext context, EditorSection next) =>
      context.go('/editor/$projectId/${next.name}');

  void _close(BuildContext context) {
    ref.read(projectListProvider.notifier).reload();
    context.go('/home');
  }

  Future<void> _rename(BuildContext context, String current) async {
    final l10n = AppLocalizations.of(context);
    final title = await askTitle(context, current);
    if (title == null || title.isEmpty) return;
    try {
      await ref.read(editorProvider(projectId).notifier).rename(title);
    } on Object catch (error) {
      if (context.mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final editor = ref.watch(editorProvider(projectId));
    final state = editor.value;
    if (state == null) {
      return Scaffold(
        body: Center(
          child: editor.hasError
              ? Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(errorMessage(l10n, editor.error!)),
                    TextButton(
                      onPressed: () => context.go('/home'),
                      child: Text(l10n.closeProject),
                    ),
                  ],
                )
              : const ProgressRing(value: null),
        ),
      );
    }
    final controller = ref.read(editorProvider(projectId).notifier);
    final transport = TransportBar(
      durationUs: state.view.durationUs.toInt(),
      playheadUs: _playheadUs,
      onUndo: state.view.canUndo ? controller.undo : null,
      onRedo: state.view.canRedo ? controller.redo : null,
    );
    final width = MediaQuery.sizeOf(context).width;
    final body = width < ShortedBreakpoints.tablet
        ? _phone(context, state, transport)
        : _wide(
            context,
            state,
            transport,
            inspector: width >= ShortedBreakpoints.desktop,
          );
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            EditorTopBar(
              title: state.view.title,
              save: state.save,
              onRename: () => _rename(context, state.view.title),
              onClose: () => _close(context),
              onExport: () => _select(context, EditorSection.export),
            ),
            Expanded(child: body),
          ],
        ),
      ),
    );
  }

  Widget _panel(EditorState state) => SectionPanel(
    section: section,
    projectId: projectId,
    media: state.view.media,
  );

  Widget _phone(BuildContext context, EditorState state, Widget transport) {
    return Column(
      children: [
        const Expanded(flex: 5, child: PreviewArea()),
        transport,
        TimelineArea(
          height: _phoneTimelineHeight,
          view: state.view,
          playheadUs: _playheadUs,
        ),
        Expanded(flex: 3, child: _panel(state)),
        SectionBar(selected: section, onSelect: (s) => _select(context, s)),
      ],
    );
  }

  Widget _wide(
    BuildContext context,
    EditorState state,
    Widget transport, {
    required bool inspector,
  }) {
    return Row(
      children: [
        SectionRail(selected: section, onSelect: (s) => _select(context, s)),
        Expanded(
          child: Column(
            children: [
              Expanded(
                child: Row(
                  children: [
                    const Expanded(child: PreviewArea()),
                    SizedBox(width: 280, child: _panel(state)),
                    if (inspector) const Inspector(),
                  ],
                ),
              ),
              transport,
              TimelineArea(
                height: _desktopTimelineHeight,
                view: state.view,
                playheadUs: _playheadUs,
              ),
            ],
          ),
        ),
      ],
    );
  }
}
