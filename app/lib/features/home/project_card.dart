import 'package:flutter/material.dart';
import 'package:intl/intl.dart';

import '../../bridge/api/projects.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../timeline/timeline_layout.dart';

enum ProjectAction { rename, duplicate, delete }

class ProjectCard extends StatelessWidget {
  const ProjectCard({
    super.key,
    required this.project,
    required this.onOpen,
    required this.onAction,
  });

  final ProjectSummary project;
  final VoidCallback onOpen;
  final ValueChanged<ProjectAction> onAction;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final locale = Localizations.localeOf(context).toString();
    final edited = DateTime.fromMillisecondsSinceEpoch(
      project.editedAtMs.toInt(),
    );
    final text = Theme.of(context).textTheme;
    return InkWell(
      borderRadius: BorderRadius.circular(ShortedRadius.m),
      onTap: onOpen,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: ShortedColors.mediaPlaceholder,
                borderRadius: BorderRadius.circular(ShortedRadius.m),
              ),
              alignment: Alignment.bottomRight,
              padding: const EdgeInsets.all(ShortedSpace.s),
              child: Text(
                formatTimecode(project.durationUs.toInt()),
                style: text.labelSmall,
              ),
            ),
          ),
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      project.title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontWeight: FontWeight.w600),
                    ),
                    Text(
                      DateFormat.yMMMd(locale).format(edited),
                      style: text.labelSmall?.copyWith(
                        color: ShortedColors.textMuted,
                      ),
                    ),
                  ],
                ),
              ),
              PopupMenuButton<ProjectAction>(
                tooltip: l10n.projectActions,
                onSelected: onAction,
                itemBuilder: (context) => [
                  PopupMenuItem(
                    value: ProjectAction.rename,
                    child: Text(l10n.rename),
                  ),
                  PopupMenuItem(
                    value: ProjectAction.duplicate,
                    child: Text(l10n.duplicate),
                  ),
                  PopupMenuItem(
                    value: ProjectAction.delete,
                    child: Text(l10n.delete),
                  ),
                ],
              ),
            ],
          ),
        ],
      ),
    );
  }
}
