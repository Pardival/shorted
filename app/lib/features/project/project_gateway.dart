import '../../bridge/api.dart' as tasks;
import '../../bridge/api/clips.dart' as clips;
import '../../bridge/api/export.dart' as exporter;
import '../../bridge/api/media.dart' as media;
import '../../bridge/api/projects.dart' as engine;
import '../../bridge/api/projects.dart';
import '../../bridge/runtime.dart';

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
  Future<ProjectView> splitClip(String id, int timeUs);
  Future<ProjectView> deleteClip(String id);
  Future<ProjectView> duplicateClip(String id);
  Future<ProjectView> moveClip(String id, int to);
  Future<ProjectView> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  });
  Stream<TaskEvent> export(String path, {required int width, required int fps});
  Stream<media.ThumbnailView> filmstrip(String mediaId);
  void moveCrop(String id, double dx, double dy, double zoomBy);
  Future<ProjectView> commitCrop(String id);
  Future<ProjectView> resetCrop(String id);
  Future<ProjectView> toggleSlowZoom(String id);
  void cancelTask(BigInt id);
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

  @override
  Future<ProjectView> splitClip(String id, int timeUs) =>
      clips.splitClip(id: id, timeUs: timeUs);

  @override
  Future<ProjectView> deleteClip(String id) => clips.deleteClip(id: id);

  @override
  Future<ProjectView> duplicateClip(String id) => clips.duplicateClip(id: id);

  @override
  Future<ProjectView> moveClip(String id, int to) =>
      clips.moveClip(id: id, to: to);

  @override
  Future<ProjectView> trimClip(
    String id, {
    required bool start,
    required int deltaUs,
  }) => clips.trimClip(
    id: id,
    edge: start ? clips.ClipEdge.start : clips.ClipEdge.end,
    deltaUs: deltaUs,
  );

  @override
  Stream<TaskEvent> export(
    String path, {
    required int width,
    required int fps,
  }) => exporter.startExport(
    path: path,
    settings: exporter.ExportSettings(width: width, fps: fps),
  );

  @override
  void moveCrop(String id, double dx, double dy, double zoomBy) =>
      clips.moveCrop(id: id, dx: dx, dy: dy, zoomBy: zoomBy);

  @override
  Future<ProjectView> commitCrop(String id) => clips.commitCrop(id: id);

  @override
  Future<ProjectView> resetCrop(String id) => clips.resetCrop(id: id);

  @override
  Future<ProjectView> toggleSlowZoom(String id) => clips.toggleSlowZoom(id: id);

  @override
  Stream<media.ThumbnailView> filmstrip(String mediaId) =>
      media.mediaFilmstrip(mediaId: mediaId);

  @override
  void cancelTask(BigInt id) => tasks.cancelTask(taskId: id);
}
