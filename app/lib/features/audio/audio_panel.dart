import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/audio.dart';
import '../../bridge/api/media.dart';
import '../../bridge/api/projects.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_gateway.dart';
import '../project/project_providers.dart';

const _audioTypes = XTypeGroup(
  label: 'audio',
  extensions: ['m4a', 'aac', 'mp3', 'wav', 'flac', 'ogg', 'opus'],
  uniformTypeIdentifiers: ['public.audio'],
);
const _maxFadeUs = 5000000.0;
const _maxVolume = 2.0;
const _maxUserTracks = 8;
const _defaultDucking = 0.6;

/// Screen S20: the original sound of clips, then every audio track.
class AudioPanel extends ConsumerWidget {
  const AudioPanel({super.key, required this.view});

  final ProjectView view;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final l10n = AppLocalizations.of(context);
    final editor = ref.read(editorProvider(view.id).notifier);

    Future<void> run(
      Future<ProjectView> Function(ProjectGateway gateway) action,
    ) async {
      try {
        await editor.apply(action);
      } on Object catch (error) {
        if (context.mounted) showToast(context, errorMessage(l10n, error));
      }
    }

    Future<void> addSound() async {
      final file = await openFile(acceptedTypeGroups: const [_audioTypes]);
      if (file == null || !context.mounted) return;
      final role = await _askRole(context);
      if (role == null) return;
      final startUs = ref.read(previewTransportProvider).positionUs;
      await run((gateway) async {
        final id = await gateway.importOne(file.path);
        if (id == null) throw StateError('not imported');
        return gateway.addAudio(id, role, startUs);
      });
    }

    final userTracks = view.sound.tracks.where((t) => t.id != null).length;
    return ListView(
      padding: const EdgeInsets.all(ShortedSpace.m),
      children: [
        _VoiceRecorder(run: run),
        const SizedBox(height: ShortedSpace.s),
        SecondaryButton(label: l10n.addAudio, onPressed: addSound),
        const SizedBox(height: ShortedSpace.s),
        SecondaryButton(
          label: l10n.addAudioTrack,
          onPressed: userTracks < _maxUserTracks
              ? () => run((gateway) => gateway.addAudioTrack())
              : null,
        ),
        for (final track in view.sound.tracks)
          _Track(
            track: track,
            timelineTrack: view.tracks.firstWhere(
              (candidate) =>
                  candidate.id == track.id &&
                  candidate.kind != TrackKindView.widgets &&
                  candidate.kind != TrackKindView.video &&
                  candidate.kind != TrackKindView.captions,
            ),
            media: view.media,
            run: run,
          ),
        _Heading(l10n.originalSound),
        for (final (index, clip) in view.sound.clips.indexed)
          SoundSettings(
            key: ValueKey(clip.clipId),
            title: l10n.clipNumber(index + 1),
            volume: clip.volume,
            fadeInUs: clip.fadeInUs.toInt(),
            fadeOutUs: clip.fadeOutUs.toInt(),
            muted: clip.muted,
            onMuted: (muted) => run(
              (gateway) => gateway.setClipSound(
                clip.clipId,
                volume: clip.volume,
                muted: muted,
                fadeInUs: clip.fadeInUs.toInt(),
                fadeOutUs: clip.fadeOutUs.toInt(),
              ),
            ),
            onChanged: (volume, fadeInUs, fadeOutUs) => run(
              (gateway) => gateway.setClipSound(
                clip.clipId,
                volume: volume,
                muted: clip.muted,
                fadeInUs: fadeInUs,
                fadeOutUs: fadeOutUs,
              ),
            ),
          ),
      ],
    );
  }
}

const _countIn = 3;

/// Count-in, then records while the video plays. The take lands where recording began.
class _VoiceRecorder extends ConsumerStatefulWidget {
  const _VoiceRecorder({required this.run});

  final _Run run;

  @override
  ConsumerState<_VoiceRecorder> createState() => _VoiceRecorderState();
}

class _VoiceRecorderState extends ConsumerState<_VoiceRecorder>
    with SingleTickerProviderStateMixin {
  late final Ticker _meter = createTicker((_) {
    setState(() => _level = ref.read(projectGatewayProvider).voiceLevel);
  });
  int? _count;
  int? _startUs;
  double _level = 0;

  @override
  void dispose() {
    _meter.dispose();
    super.dispose();
  }

  Future<void> _record() async {
    final l10n = AppLocalizations.of(context);
    for (var count = _countIn; count > 0; count--) {
      setState(() => _count = count);
      await Future<void>.delayed(const Duration(seconds: 1));
      if (!mounted) return;
    }
    setState(() => _count = null);
    final gateway = ref.read(projectGatewayProvider);
    try {
      await gateway.startVoiceOver();
    } on Object {
      if (mounted) showToast(context, l10n.micUnavailable);
      return;
    }
    if (!mounted) return;
    setState(() => _startUs = ref.read(previewTransportProvider).positionUs);
    ref.read(recordingVoiceProvider.notifier).set(true);
    await _meter.start();
  }

  Future<void> _stop() async {
    final startUs = _startUs;
    if (startUs == null) return;
    _meter.stop();
    ref.read(recordingVoiceProvider.notifier).set(false);
    setState(() => _startUs = null);
    await widget.run((gateway) => gateway.stopVoiceOver(startUs));
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final count = _count;
    if (count != null) {
      return Center(
        child: Text('$count', style: Theme.of(context).textTheme.displaySmall),
      );
    }
    if (_startUs == null) {
      return PrimaryButton(label: l10n.recordVoiceOver, onPressed: _record);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Semantics(
          label: l10n.recordingLevel,
          child: LinearProgressIndicator(value: _level),
        ),
        const SizedBox(height: ShortedSpace.s),
        PrimaryButton(label: l10n.stopRecording, onPressed: _stop),
      ],
    );
  }
}

Future<AudioRoleView?> _askRole(BuildContext context) {
  final l10n = AppLocalizations.of(context);
  return showDialog<AudioRoleView>(
    context: context,
    builder: (context) => SimpleDialog(
      title: Text(l10n.chooseAudioRole),
      children: [
        for (final (role, label) in [
          (AudioRoleView.importedVoice, l10n.roleVoice),
          (AudioRoleView.music, l10n.roleMusic),
          (AudioRoleView.soundEffect, l10n.roleEffect),
        ])
          SimpleDialogOption(
            onPressed: () => Navigator.pop(context, role),
            child: Text(label),
          ),
      ],
    ),
  );
}

class _Heading extends StatelessWidget {
  const _Heading(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: ShortedSpace.m),
    child: Text(text, style: const TextStyle(fontWeight: FontWeight.w600)),
  );
}

typedef _Run = Future<void> Function(
  Future<ProjectView> Function(ProjectGateway gateway) action,
);

class _Track extends StatelessWidget {
  const _Track({
    required this.track,
    required this.timelineTrack,
    required this.media,
    required this.run,
  });

  final AudioTrackView track;
  final TrackView timelineTrack;
  final List<MediaItemView> media;
  final _Run run;

  String _nameOf(AudioItemView item) {
    for (final entry in media) {
      if (entry.id == item.mediaId) return entry.name;
    }
    return '';
  }

  Future<void> _remove(BuildContext context, String id) async {
    final l10n = AppLocalizations.of(context);
    if (track.items.isNotEmpty) {
      final sure = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(l10n.removeTrack),
          content: Text(l10n.removeTrackBody),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(l10n.cancel),
            ),
            TextButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text(l10n.delete),
            ),
          ],
        ),
      );
      if (sure != true) return;
    }
    await run((gateway) => gateway.removeAudioTrack(id));
  }

  Future<void> _rename(BuildContext context, String id) async {
    final name = await askTitle(context, track.name ?? '');
    if (name != null) {
      await run((gateway) => gateway.renameAudioTrack(id, name));
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final id = track.id;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(child: _Heading(track.name ?? l10n.trackVoiceOver)),
            ShortedIconButton(
              icon: track.muted ? Icons.volume_off : Icons.volume_up,
              tooltip: l10n.mute,
              onPressed: () => run(
                (gateway) =>
                    gateway.setTrackFlags(timelineTrack, muted: !track.muted),
              ),
            ),
            if (id != null) ...[
              ShortedIconButton(
                icon: Icons.edit_outlined,
                tooltip: l10n.renameTrack,
                onPressed: () => _rename(context, id),
              ),
              ShortedIconButton(
                icon: Icons.delete_outline,
                tooltip: l10n.removeTrack,
                onPressed: () => _remove(context, id),
              ),
            ],
          ],
        ),
        for (final item in track.items)
          SoundSettings(
            key: ValueKey(item.id),
            title: _nameOf(item),
            volume: item.volume,
            fadeInUs: item.fadeInUs.toInt(),
            fadeOutUs: item.fadeOutUs.toInt(),
            looped: item.looped,
            onLooped: (looped) =>
                run((gateway) => gateway.setAudioLoop(item.id, looped: looped)),
            onDelete: () => run((gateway) => gateway.deleteAudio(item.id)),
            ducking: item.role == AudioRoleView.music ? item.ducking : null,
            showDucking: item.role == AudioRoleView.music,
            onDucking: (strength) =>
                run((gateway) => gateway.setDucking(item.id, strength)),
            onChanged: (volume, fadeInUs, fadeOutUs) => run(
              (gateway) => gateway.setAudioSound(
                item.id,
                volume: volume,
                fadeInUs: fadeInUs,
                fadeOutUs: fadeOutUs,
              ),
            ),
          ),
      ],
    );
  }
}

/// Sliders follow the finger locally; the engine hears once, when the finger lifts.
class SoundSettings extends StatefulWidget {
  const SoundSettings({
    super.key,
    required this.title,
    required this.volume,
    required this.fadeInUs,
    required this.fadeOutUs,
    required this.onChanged,
    this.muted,
    this.onMuted,
    this.looped,
    this.onLooped,
    this.onDelete,
    this.ducking,
    this.showDucking = false,
    this.onDucking,
  });

  final String title;
  final double volume;
  final int fadeInUs;
  final int fadeOutUs;
  final void Function(double volume, int fadeInUs, int fadeOutUs) onChanged;
  final bool? muted;
  final ValueChanged<bool>? onMuted;
  final bool? looped;
  final ValueChanged<bool>? onLooped;
  final VoidCallback? onDelete;

  /// Music only: how deep it dips under speech, null when it does not.
  final double? ducking;
  final bool showDucking;
  final ValueChanged<double?>? onDucking;

  @override
  State<SoundSettings> createState() => SoundSettingsState();
}

class SoundSettingsState extends State<SoundSettings> {
  double? _volume;
  double? _fadeIn;
  double? _fadeOut;

  void _commit() => widget.onChanged(
    _volume ?? widget.volume,
    (_fadeIn ?? widget.fadeInUs.toDouble()).round(),
    (_fadeOut ?? widget.fadeOutUs.toDouble()).round(),
  );

  Widget _slider(
    String label,
    double value,
    double max,
    ValueChanged<double> onChanged,
  ) => Row(
    children: [
      SizedBox(
        width: 96,
        child: Text(
          label,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: ShortedColors.textMuted),
        ),
      ),
      Expanded(
        child: Slider(
          value: value.clamp(0, max),
          max: max,
          label: label,
          onChanged: onChanged,
          onChangeEnd: (_) => _commit(),
        ),
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final muted = widget.muted;
    final looped = widget.looped;
    final onDelete = widget.onDelete;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(widget.title, overflow: TextOverflow.ellipsis),
            ),
            if (muted != null)
              ShortedIconButton(
                icon: muted ? Icons.volume_off : Icons.volume_up,
                tooltip: l10n.mute,
                onPressed: () => widget.onMuted?.call(!muted),
              ),
            if (looped != null)
              ShortedIconButton(
                icon: looped ? Icons.repeat_on : Icons.repeat,
                tooltip: l10n.loopAudio,
                onPressed: () => widget.onLooped?.call(!looped),
              ),
            if (onDelete != null)
              ShortedIconButton(
                icon: Icons.delete_outline,
                tooltip: l10n.delete,
                onPressed: onDelete,
              ),
          ],
        ),
        if (widget.showDucking)
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l10n.duckUnderSpeech),
            value: widget.ducking != null,
            onChanged: (on) =>
                widget.onDucking?.call(on ? _defaultDucking : null),
          ),
        _slider(
          l10n.volume,
          _volume ?? widget.volume,
          _maxVolume,
          (value) => setState(() => _volume = value),
        ),
        _slider(
          l10n.fadeIn,
          _fadeIn ?? widget.fadeInUs.toDouble(),
          _maxFadeUs,
          (value) => setState(() => _fadeIn = value),
        ),
        _slider(
          l10n.fadeOut,
          _fadeOut ?? widget.fadeOutUs.toDouble(),
          _maxFadeUs,
          (value) => setState(() => _fadeOut = value),
        ),
      ],
    );
  }
}

/// The sound settings of a clip or a sound of the timeline, or null for anything else (T-1203).
Widget? soundSettingsFor(
  BuildContext context,
  ProjectView view,
  String id,
  Future<void> Function(
    Future<ProjectView> Function(ProjectGateway gateway) action,
  )
  run,
) {
  final l10n = AppLocalizations.of(context);
  for (final (index, clip) in view.sound.clips.indexed) {
    if (clip.clipId != id) continue;
    return SoundSettings(
      key: ValueKey(clip.clipId),
      title: l10n.originalSoundOf(index + 1),
      volume: clip.volume,
      fadeInUs: clip.fadeInUs.toInt(),
      fadeOutUs: clip.fadeOutUs.toInt(),
      muted: clip.muted,
      onMuted: (muted) => run(
        (gateway) => gateway.setClipSound(
          clip.clipId,
          volume: clip.volume,
          muted: muted,
          fadeInUs: clip.fadeInUs.toInt(),
          fadeOutUs: clip.fadeOutUs.toInt(),
        ),
      ),
      onChanged: (volume, fadeInUs, fadeOutUs) => run(
        (gateway) => gateway.setClipSound(
          clip.clipId,
          volume: volume,
          muted: clip.muted,
          fadeInUs: fadeInUs,
          fadeOutUs: fadeOutUs,
        ),
      ),
    );
  }
  for (final track in view.sound.tracks) {
    for (final item in track.items) {
      if (item.id != id) continue;
      var name = '';
      for (final entry in view.media) {
        if (entry.id == item.mediaId) name = entry.name;
      }
      return SoundSettings(
        key: ValueKey(item.id),
        title: name,
        volume: item.volume,
        fadeInUs: item.fadeInUs.toInt(),
        fadeOutUs: item.fadeOutUs.toInt(),
        looped: item.looped,
        onLooped: (looped) =>
            run((gateway) => gateway.setAudioLoop(item.id, looped: looped)),
        ducking: item.role == AudioRoleView.music ? item.ducking : null,
        showDucking: item.role == AudioRoleView.music,
        onDucking: (strength) =>
            run((gateway) => gateway.setDucking(item.id, strength)),
        onChanged: (volume, fadeInUs, fadeOutUs) => run(
          (gateway) => gateway.setAudioSound(
            item.id,
            volume: volume,
            fadeInUs: fadeInUs,
            fadeOutUs: fadeOutUs,
          ),
        ),
      );
    }
  }
  return null;
}

/// Opens the sound settings of a clip or sound selected on the timeline, kept up to date.
Future<void> showSoundSheet(
  BuildContext context,
  String projectId,
  String id,
) => showModalBottomSheet<void>(
  context: context,
  showDragHandle: true,
  builder: (context) => Consumer(
    builder: (context, ref, _) {
      final view = ref.watch(editorProvider(projectId)).value?.view;
      if (view == null) return const SizedBox.shrink();
      final l10n = AppLocalizations.of(context);
      Future<void> run(
        Future<ProjectView> Function(ProjectGateway gateway) action,
      ) async {
        try {
          await ref.read(editorProvider(projectId).notifier).apply(action);
        } on Object catch (error) {
          if (context.mounted) {
            showToast(context, errorMessage(l10n, error));
          }
        }
      }

      final settings = soundSettingsFor(context, view, id, run);
      if (settings == null) return const SizedBox.shrink();
      return SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(
            ShortedSpace.m,
            0,
            ShortedSpace.m,
            ShortedSpace.m,
          ),
          child: settings,
        ),
      );
    },
  ),
);
