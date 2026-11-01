import '../../bridge/api/media.dart' as media;
import '../../bridge/api/projects.dart' as engine;
import '../../bridge/api/projects.dart';

/// Everything the app asks of the engine about projects. Screens use this, so widget tests can
/// run on a fake (docs/09 §1).
abstract interface class ProjectGateway {
  Future<List<ProjectSummary>> list();
  Future<List<ProjectSummary>> trash();
  Future<String> create(String title);
  Future<void> open(String id);
  Future<void> close();
  Future<String> duplicate(String id, String title);
  Future<void> moveToTrash(String id);
  Future<void> restore(String id);
  Future<void> rename(String id, String title);
  Future<ProjectView> view();
  Future<ProjectView> undo();
  Future<ProjectView> redo();
  Stream<SaveStateView> saveEvents();

  /// Returns how many files failed; the others are imported.
  Future<int> importFiles(List<String> paths);
  Future<ProjectView> placeClips(List<String> mediaIds);
  Future<ProjectView> relink(String mediaId, String path);
}

class EngineGateway implements ProjectGateway {
  EngineGateway(this.root);

  final String root;

  @override
  Future<List<ProjectSummary>> list() => engine.listProjects(root: root);

  @override
  Future<List<ProjectSummary>> trash() => engine.listTrash(root: root);

  @override
  Future<String> create(String title) =>
      engine.createProject(root: root, title: title);

  @override
  Future<void> open(String id) => engine.openProject(root: root, id: id);

  @override
  Future<void> close() => engine.closeProject();

  @override
  Future<String> duplicate(String id, String title) =>
      engine.duplicateProject(root: root, id: id, title: title);

  @override
  Future<void> moveToTrash(String id) =>
      engine.trashProject(root: root, id: id);

  @override
  Future<void> restore(String id) => engine.restoreProject(root: root, id: id);

  @override
  Future<void> rename(String id, String title) =>
      engine.renameProject(root: root, id: id, title: title);

  @override
  Future<ProjectView> view() => engine.projectView();

  @override
  Future<ProjectView> undo() => engine.undo();

  @override
  Future<ProjectView> redo() => engine.redo();

  @override
  Stream<SaveStateView> saveEvents() => engine.saveEvents();

  @override
  Future<int> importFiles(List<String> paths) async {
    final outcomes = await media.importFiles(paths: paths);
    return outcomes.where((outcome) => outcome.error != null).length;
  }

  @override
  Future<ProjectView> placeClips(List<String> mediaIds) =>
      media.placeClips(mediaIds: mediaIds);

  @override
  Future<ProjectView> relink(String mediaId, String path) =>
      media.relinkMedia(id: mediaId, path: path);
}
