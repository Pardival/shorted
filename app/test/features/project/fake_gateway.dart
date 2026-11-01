import 'dart:async';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:shorted/bridge/api/projects.dart';
import 'package:shorted/bridge/error.dart';
import 'package:shorted/features/project/project_gateway.dart';

PlatformInt64 big(int value) => PlatformInt64Util.from(value);

class FakeProject {
  FakeProject(this.id, this.title);

  final String id;
  String title;
  final titles = <String>[];
  final redone = <String>[];
}

/// In-memory stand-in for the engine.
class FakeGateway implements ProjectGateway {
  final projects = <FakeProject>[];
  final trashed = <FakeProject>[];
  final saves = StreamController<SaveStateView>.broadcast();
  ErrorCode? failOpenWith;
  FakeProject? opened;
  var _next = 0;

  ProjectSummary _summary(FakeProject project) => ProjectSummary(
    id: project.id,
    title: project.title,
    durationUs: big(0),
    editedAtMs: big(1793000000000),
  );

  FakeProject _find(String id) => projects.firstWhere((p) => p.id == id);

  @override
  Future<List<ProjectSummary>> list() async => projects.map(_summary).toList();

  @override
  Future<List<ProjectSummary>> trash() async => trashed.map(_summary).toList();

  @override
  Future<String> create(String title) async {
    final project = FakeProject('p${_next++}', title);
    projects.add(project);
    opened = project;
    return project.id;
  }

  @override
  Future<void> open(String id) async {
    final code = failOpenWith;
    if (code != null) throw EngineFailure(code: code);
    opened = _find(id);
  }

  @override
  Future<void> close() async => opened = null;

  @override
  Future<String> duplicate(String id, String title) async {
    final project = FakeProject('p${_next++}', title);
    projects.add(project);
    return project.id;
  }

  @override
  Future<void> moveToTrash(String id) async {
    final project = _find(id);
    projects.remove(project);
    trashed.add(project);
  }

  @override
  Future<void> restore(String id) async {
    final project = trashed.firstWhere((p) => p.id == id);
    trashed.remove(project);
    projects.add(project);
  }

  @override
  Future<void> rename(String id, String title) async {
    final project = _find(id);
    project.titles.add(project.title);
    project.redone.clear();
    project.title = title;
  }

  @override
  Future<ProjectView> view() async {
    final project = opened!;
    return ProjectView(
      id: project.id,
      title: project.title,
      durationUs: big(4000000),
      canUndo: project.titles.isNotEmpty,
      canRedo: project.redone.isNotEmpty,
      tracks: [
        TrackView(
          kind: TrackKindView.video,
          items: [
            TrackItemView(id: 'c1', startUs: big(0), endUs: big(4000000)),
          ],
        ),
      ],
    );
  }

  @override
  Future<ProjectView> undo() async {
    final project = opened!;
    project.redone.add(project.title);
    project.title = project.titles.removeLast();
    return view();
  }

  @override
  Future<ProjectView> redo() async {
    final project = opened!;
    project.titles.add(project.title);
    project.title = project.redone.removeLast();
    return view();
  }

  @override
  Stream<SaveStateView> saveEvents() => saves.stream;
}
