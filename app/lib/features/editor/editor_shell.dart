import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/tokens.g.dart';
import 'editor_parts.dart';
import 'editor_section.dart';

const _phoneTimelineHeight = 72.0;
const _desktopTimelineHeight = 220.0;

/// The section lives in the route, so resizing the window keeps it (T-0202).
class EditorShell extends StatelessWidget {
  const EditorShell({
    super.key,
    required this.projectId,
    required this.section,
  });

  final String projectId;
  final EditorSection section;

  void _select(BuildContext context, EditorSection next) =>
      context.go('/editor/$projectId/${next.name}');

  @override
  Widget build(BuildContext context) {
    final width = MediaQuery.sizeOf(context).width;
    final body = width < ShortedBreakpoints.tablet
        ? _phone(context)
        : _wide(context, inspector: width >= ShortedBreakpoints.desktop);
    return Scaffold(
      body: SafeArea(
        child: Column(
          children: [
            EditorTopBar(
              onClose: () => context.go('/home'),
              onExport: () => _select(context, EditorSection.export),
            ),
            Expanded(child: body),
          ],
        ),
      ),
    );
  }

  Widget _phone(BuildContext context) {
    return Column(
      children: [
        const Expanded(flex: 5, child: PreviewArea()),
        const TransportBar(),
        const TimelineArea(height: _phoneTimelineHeight),
        Expanded(flex: 3, child: SectionPanel(section: section)),
        SectionBar(selected: section, onSelect: (s) => _select(context, s)),
      ],
    );
  }

  Widget _wide(BuildContext context, {required bool inspector}) {
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
                    SizedBox(width: 280, child: SectionPanel(section: section)),
                    if (inspector) const Inspector(),
                  ],
                ),
              ),
              const TransportBar(),
              const TimelineArea(height: _desktopTimelineHeight),
            ],
          ),
        ),
      ],
    );
  }
}
