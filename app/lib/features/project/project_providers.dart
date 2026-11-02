import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/projects.dart';
import 'project_gateway.dart';

/// Overridden at startup, once the data folder is known.
final projectGatewayProvider = Provider<ProjectGateway>(
  (ref) => throw UnimplementedError('projectGatewayProvider is not set'),
);

final projectListProvider =
    AsyncNotifierProvider<ProjectList, List<ProjectSummary>>(ProjectList.new);

class ProjectList extends AsyncNotifier<List<ProjectSummary>> {
  ProjectGateway get _gateway => ref.read(projectGatewayProvider);

  @override
  Future<List<ProjectSummary>> build() => _gateway.list();

  Future<void> _refresh() async {
    state = AsyncData(await _gateway.list());
  }

  Future<String> create(String title) async {
    final id = await _gateway.create(title);
    await _refresh();
    return id;
  }

  Future<void> rename(String id, String title) async {
    await _gateway.rename(id, title);
    await _refresh();
  }

  Future<void> duplicate(String id, String title) async {
    await _gateway.duplicate(id, title);
    await _refresh();
  }

  Future<void> moveToTrash(String id) async {
    await _gateway.moveToTrash(id);
    await _refresh();
  }

  Future<void> restore(String id) async {
    await _gateway.restore(id);
    await _refresh();
  }

  Future<void> reload() => _refresh();
}

final trashProvider = FutureProvider.autoDispose<List<ProjectSummary>>(
  (ref) => ref.read(projectGatewayProvider).trash(),
);

class EditorState {
  const EditorState({required this.view, required this.save});

  final ProjectView view;
  final SaveStateView save;

  EditorState copyWith({ProjectView? view, SaveStateView? save}) =>
      EditorState(view: view ?? this.view, save: save ?? this.save);
}

final editorProvider = AsyncNotifierProvider.autoDispose
    .family<EditorController, EditorState, String>(EditorController.new);

/// The open project. The engine owns the data: every action returns a new view from it.
class EditorController extends AsyncNotifier<EditorState> {
  EditorController(this.projectId);

  final String projectId;

  ProjectGateway get _gateway => ref.read(projectGatewayProvider);

  @override
  Future<EditorState> build() async {
    final saves = _gateway.saveEvents().listen((save) {
      final current = state.value;
      if (current != null) state = AsyncData(current.copyWith(save: save));
    });
    ref.onDispose(saves.cancel);
    await _gateway.open(projectId);
    return EditorState(view: await _gateway.view(), save: SaveStateView.saved);
  }

  void _show(ProjectView view) {
    final current = state.value;
    if (current != null) state = AsyncData(current.copyWith(view: view));
  }

  Future<void> undo() async => _show(await _gateway.undo());

  Future<void> redo() async => _show(await _gateway.redo());

  Future<void> rename(String title) async {
    await _gateway.rename(projectId, title);
    _show(await _gateway.view());
  }

  /// Returns how many files failed.
  Future<int> importFiles(List<String> paths) async {
    final failed = await _gateway.importFiles(paths);
    _show(await _gateway.view());
    return failed;
  }

  Future<void> placeClips(List<String> mediaIds) async =>
      _show(await _gateway.placeClips(mediaIds));

  Future<void> relink(String mediaId, String path) async =>
      _show(await _gateway.relink(mediaId, path));
}
