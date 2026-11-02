import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../bridge/api/projects.dart';
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

const _oneSecondUs = 1000000;
const _oneFrameUs = 33333;
const _zoomStep = 1.25;

class _EditorShellState extends ConsumerState<EditorShell> {
  final _playheadUs = ValueNotifier<int>(0);
  final _transport = GlobalKey<TransportBarState>();
  final _zoomRequests = StreamController<double>.broadcast();

  String get projectId => widget.projectId;
  EditorSection get section => widget.section;

  @override
  void dispose() {
    _playheadUs.dispose();
    unawaited(_zoomRequests.close());
    super.dispose();
  }

  void _select(BuildContext context, EditorSection next) =>
      context.go('/editor/$projectId/${next.name}');

  void _close(BuildContext context) {
    ref.read(projectListProvider.notifier).reload();
    context.go('/home');
  }

  void _nudge(int deltaUs, int durationUs) {
    _playheadUs.value = (_playheadUs.value + deltaUs).clamp(0, durationUs);
  }

  /// The selected clip, or else the one under the playhead.
  String? _clipToSplit(EditorState state) {
    final selected = ref.read(selectedClipProvider);
    if (selected != null) return selected;
    final time = _playheadUs.value;
    for (final track in state.view.tracks) {
      if (track.kind != TrackKindView.video) continue;
      for (final item in track.items) {
        if (time >= item.startUs.toInt() && time < item.endUs.toInt()) {
          return item.id;
        }
      }
    }
    return null;
  }

  Future<void> _guarded(Future<void> Function() action) async {
    final l10n = AppLocalizations.of(context);
    try {
      await action();
    } on Object catch (error) {
      if (mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  Map<ShortcutActivator, VoidCallback> _shortcuts(
    EditorState state,
    EditorController controller,
  ) {
    final durationUs = state.view.durationUs.toInt();
    void split() {
      final id = _clipToSplit(state);
      if (id != null) {
        _guarded(() => controller.splitClip(id, _playheadUs.value));
      }
    }

    void remove() {
      final id = ref.read(selectedClipProvider);
      if (id == null) return;
      ref.read(selectedClipProvider.notifier).set(null);
      _guarded(() => controller.deleteClip(id));
    }

    void undo() {
      if (state.view.canUndo) _guarded(controller.undo);
    }

    void redo() {
      if (state.view.canRedo) _guarded(controller.redo);
    }

    return {
      const SingleActivator(LogicalKeyboardKey.space): () =>
          _transport.currentState?.toggle(),
      const CharacterActivator('k'): () => _transport.currentState?.pause(),
      const CharacterActivator('j'): () => _nudge(-_oneSecondUs, durationUs),
      const CharacterActivator('l'): () => _nudge(_oneSecondUs, durationUs),
      const CharacterActivator('s'): split,
      const SingleActivator(LogicalKeyboardKey.delete): remove,
      const SingleActivator(LogicalKeyboardKey.backspace): remove,
      const SingleActivator(LogicalKeyboardKey.arrowLeft): () =>
          _nudge(-_oneFrameUs, durationUs),
      const SingleActivator(LogicalKeyboardKey.arrowRight): () =>
          _nudge(_oneFrameUs, durationUs),
      const CharacterActivator('+'): () => _zoomRequests.add(_zoomStep),
      const CharacterActivator('='): () => _zoomRequests.add(_zoomStep),
      const CharacterActivator('-'): () => _zoomRequests.add(1 / _zoomStep),
      const SingleActivator(LogicalKeyboardKey.keyZ, meta: true): undo,
      const SingleActivator(LogicalKeyboardKey.keyZ, control: true): undo,
      const SingleActivator(LogicalKeyboardKey.keyZ, meta: true, shift: true):
          redo,
      const SingleActivator(
        LogicalKeyboardKey.keyZ,
        control: true,
        shift: true,
      ): redo,
    };
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
      key: _transport,
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
            Expanded(
              child: CallbackShortcuts(
                bindings: _shortcuts(state, controller),
                child: Focus(autofocus: true, child: body),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _panel(EditorState state) =>
      SectionPanel(section: section, projectId: projectId, view: state.view);

  Widget _phone(BuildContext context, EditorState state, Widget transport) {
    return Column(
      children: [
        const Expanded(flex: 5, child: PreviewArea()),
        transport,
        TimelineArea(
          height: _phoneTimelineHeight,
          view: state.view,
          playheadUs: _playheadUs,
          zoomRequests: _zoomRequests.stream,
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
                zoomRequests: _zoomRequests.stream,
              ),
            ],
          ),
        ),
      ],
    );
  }
}
