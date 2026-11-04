import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';
import 'project_card.dart';

const _cardMaxWidth = 200.0;
const _cardAspectRatio = 0.6;

const _bundleType = XTypeGroup(label: 'Shorted', extensions: ['shorted']);

class HomeScreen extends ConsumerWidget {
  const HomeScreen({super.key});

  Future<void> _guard(
    BuildContext context,
    Future<void> Function() action,
  ) async {
    final l10n = AppLocalizations.of(context);
    try {
      await action();
    } on Object catch (error) {
      if (context.mounted) showToast(context, errorMessage(l10n, error));
    }
  }

  /// A project title as a file name: letters, digits, spaces and dashes only.
  static String _fileName(String title) {
    final kept = title.replaceAll(
      RegExp(r'[^\p{L}\p{N} _-]', unicode: true),
      '',
    );
    return kept.trim().isEmpty ? 'project' : kept.trim();
  }

  Future<void> _import(BuildContext context, WidgetRef ref) async {
    final file = await openFile(acceptedTypeGroups: const [_bundleType]);
    if (file == null || !context.mounted) return;
    final l10n = AppLocalizations.of(context);
    await _guard(context, () async {
      await ref.read(projectListProvider.notifier).importBundle(file.path);
      if (context.mounted) showToast(context, l10n.bundleImported);
    });
  }

  Future<void> _create(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    return _guard(context, () async {
      final list = ref.read(projectListProvider.notifier);
      final id = await list.create(l10n.untitledProject);
      if (context.mounted) context.go('/editor/$id/clips');
    });
  }

  Future<void> _act(
    BuildContext context,
    WidgetRef ref,
    ProjectSummary project,
    ProjectAction action,
  ) async {
    final l10n = AppLocalizations.of(context);
    final list = ref.read(projectListProvider.notifier);
    switch (action) {
      case ProjectAction.rename:
        final title = await askTitle(context, project.title);
        if (title == null || title.isEmpty || !context.mounted) return;
        return _guard(context, () => list.rename(project.id, title));
      case ProjectAction.duplicate:
        final title = l10n.copyOf(project.title);
        return _guard(context, () => list.duplicate(project.id, title));
      case ProjectAction.export:
        final location = await getSaveLocation(
          suggestedName: '${_fileName(project.title)}.shorted',
          acceptedTypeGroups: const [_bundleType],
        );
        if (location == null || !context.mounted) return;
        await _guard(context, () async {
          await list.exportBundle(project.id, location.path);
          if (context.mounted) showToast(context, l10n.bundleExported);
        });
      case ProjectAction.delete:
        if (!await confirmDelete(context, project.title)) return;
        if (!context.mounted) return;
        await _guard(context, () => list.moveToTrash(project.id));
        if (!context.mounted) return;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(l10n.movedToTrash),
            action: SnackBarAction(
              label: l10n.restore,
              onPressed: () => list.restore(project.id),
            ),
          ),
        );
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final text = Theme.of(context).textTheme;
    final projects = ref.watch(projectListProvider);
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(ShortedSpace.xl),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(l10n.appTitle, style: text.displaySmall),
                  ),
                  ShortedIconButton(
                    icon: Icons.file_open_outlined,
                    tooltip: l10n.importBundle,
                    onPressed: () => _import(context, ref),
                  ),
                  ShortedIconButton(
                    icon: Icons.delete_outline,
                    tooltip: l10n.trash,
                    onPressed: () => showTrash(context),
                  ),
                  ShortedIconButton(
                    icon: Icons.settings_outlined,
                    tooltip: l10n.settingsTitle,
                    onPressed: () => context.go('/settings'),
                  ),
                ],
              ),
              const SizedBox(height: ShortedSpace.xl),
              PrimaryButton(
                label: l10n.newShort,
                icon: Icons.add,
                onPressed: () => _create(context, ref),
              ),
              const SizedBox(height: ShortedSpace.xl),
              Expanded(
                child: switch (projects) {
                  AsyncData(:final value) when value.isEmpty =>
                    const _EmptyHome(),
                  AsyncData(:final value) => GridView.builder(
                    gridDelegate:
                        const SliverGridDelegateWithMaxCrossAxisExtent(
                          maxCrossAxisExtent: _cardMaxWidth,
                          childAspectRatio: _cardAspectRatio,
                          mainAxisSpacing: ShortedSpace.m,
                          crossAxisSpacing: ShortedSpace.m,
                        ),
                    itemCount: value.length,
                    itemBuilder: (context, index) {
                      final project = value[index];
                      return ProjectCard(
                        project: project,
                        onOpen: () => context.go('/editor/${project.id}/clips'),
                        onAction: (action) =>
                            _act(context, ref, project, action),
                      );
                    },
                  ),
                  AsyncError(:final error) => Center(
                    child: Text(errorMessage(l10n, error)),
                  ),
                  _ => const Center(child: ProgressRing(value: null)),
                },
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _EmptyHome extends StatelessWidget {
  const _EmptyHome();

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            l10n.emptyHomeTitle,
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: ShortedSpace.s),
          Text(
            l10n.emptyHomeBody,
            textAlign: TextAlign.center,
            style: const TextStyle(color: ShortedColors.textMuted),
          ),
        ],
      ),
    );
  }
}

void showTrash(BuildContext context) {
  showModalBottomSheet<void>(
    context: context,
    builder: (context) => const _TrashSheet(),
  );
}

class _TrashSheet extends ConsumerWidget {
  const _TrashSheet();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final trash = ref.watch(trashProvider).value ?? const [];
    return SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(ShortedSpace.l),
            child: Text(
              l10n.trash,
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ),
          if (trash.isEmpty)
            Padding(
              padding: const EdgeInsets.all(ShortedSpace.l),
              child: Text(l10n.trashEmpty),
            ),
          for (final project in trash)
            ListTile(
              title: Text(project.title),
              trailing: TextButton(
                onPressed: () async {
                  await ref
                      .read(projectListProvider.notifier)
                      .restore(project.id);
                  ref.invalidate(trashProvider);
                },
                child: Text(l10n.restore),
              ),
            ),
        ],
      ),
    );
  }
}
