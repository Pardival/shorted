import 'package:flutter/material.dart';

import '../../l10n/generated/app_localizations.dart';

enum EditorSection {
  clips(Icons.movie_outlined),
  audio(Icons.graphic_eq),
  captions(Icons.subtitles_outlined),
  enhance(Icons.auto_awesome_outlined),
  export(Icons.ios_share);

  const EditorSection(this.icon);

  final IconData icon;

  static EditorSection parse(String? name) => EditorSection.values.firstWhere(
    (section) => section.name == name,
    orElse: () => EditorSection.clips,
  );

  String label(AppLocalizations l10n) => switch (this) {
    EditorSection.clips => l10n.sectionClips,
    EditorSection.audio => l10n.sectionAudio,
    EditorSection.captions => l10n.sectionCaptions,
    EditorSection.enhance => l10n.sectionEnhance,
    EditorSection.export => l10n.sectionExport,
  };
}
