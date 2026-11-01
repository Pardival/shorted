import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';

import '../../l10n/generated/app_localizations.dart';
import 'frame_stats.dart';

const _channel = MethodChannel('shorted/spike');
const _renderWidth = 1080;
const _renderHeight = 1920;
const _warmUp = Duration(seconds: 2);
const _window = Duration(seconds: 15);
const _budgetMs = 16.0;
const _chipSize = 96.0;

enum SpikeMode { zeroCopy, pixelCopy }

/// From the frames' own timestamps: batches make their count unreliable against a wall clock.
double _framesPerSecond(List<FrameTiming> frames) {
  if (frames.length < 2) return 0;
  int start(FrameTiming f) => f.timestampInMicroseconds(FramePhase.vsyncStart);
  final span = start(frames.last) - start(frames.first);
  return span == 0 ? 0 : (frames.length - 1) * 1e6 / span;
}

/// T-0005 measurement: shows the engine texture, drags a Flutter widget over it, and writes a
/// JSON report for each texture path.
class SpikeScreen extends StatefulWidget {
  const SpikeScreen({super.key, this.exitWhenDone = false});

  final bool exitWhenDone;

  @override
  State<SpikeScreen> createState() => _SpikeScreenState();
}

class _SpikeScreenState extends State<SpikeScreen>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker = createTicker(_onTick);
  final _chipOffset = ValueNotifier<Offset>(Offset.zero);
  final _frameTimes = <FrameTiming>[];
  final _preview = GlobalKey();
  int? _textureId;
  SpikeMode? _mode;
  String? _reportPath;

  @override
  void initState() {
    super.initState();
    _ticker.start();
    unawaited(_run());
  }

  @override
  void dispose() {
    _ticker.dispose();
    _chipOffset.dispose();
    super.dispose();
  }

  void _onTick(Duration elapsed) {
    final t = elapsed.inMicroseconds / 1e6;
    _chipOffset.value = Offset(math.sin(t * 2), math.cos(t * 1.3));
  }

  void _onTimings(List<FrameTiming> timings) => _frameTimes.addAll(timings);

  Future<Map<String, Object>> _measure(SpikeMode mode) async {
    final id = await _channel.invokeMethod<int>('start', {
      'width': _renderWidth,
      'height': _renderHeight,
      'zeroCopy': mode == SpikeMode.zeroCopy,
    });
    setState(() {
      _mode = mode;
      _textureId = id;
    });
    await Future<void>.delayed(_warmUp);
    await _channel.invokeMethod<Object>('stats');
    _frameTimes.clear();
    final watch = Stopwatch()..start();
    await Future<void>.delayed(_window);
    final native = await _channel.invokeMapMethod<String, Object?>('stats');
    final seconds = watch.elapsedMicroseconds / 1e6;
    // Timings arrive in batches; wait for the last ones of the window.
    await Future<void>.delayed(const Duration(seconds: 1));
    final frames = List.of(_frameTimes);
    await _savePreview(mode);
    await _channel.invokeMethod<void>('stop');
    setState(() => _textureId = null);

    double ms(Duration value) => value.inMicroseconds / 1000;
    final renderMs = (native?['renderMs'] as List<Object?>? ?? [])
        .cast<double>();
    final over = frames.where(
      (f) =>
          ms(f.buildDuration) > _budgetMs || ms(f.rasterDuration) > _budgetMs,
    );
    return {
      'mode': mode.name,
      'seconds': seconds,
      'engineRenderedFps': (native?['rendered'] as int? ?? 0) / seconds,
      'texturePresentedFps': (native?['presented'] as int? ?? 0) / seconds,
      'engineSkipped': native?['skipped'] as int? ?? 0,
      'engineFailed': native?['failed'] as int? ?? 0,
      'engineRenderMs': DurationStats.of(renderMs).toJson(),
      'uiFps': _framesPerSecond(frames),
      'uiBuildMs': DurationStats.of(frames.map((f) => ms(f.buildDuration)))
          .toJson(),
      'uiRasterMs': DurationStats.of(frames.map((f) => ms(f.rasterDuration)))
          .toJson(),
      'uiFramesOverBudget': over.length,
    };
  }

  /// Saves what Flutter composes, after the window so it cannot affect the measurements.
  Future<void> _savePreview(SpikeMode mode) async {
    final boundary =
        _preview.currentContext?.findRenderObject() as RenderRepaintBoundary?;
    final image = await boundary?.toImage();
    final bytes = await image?.toByteData(format: ImageByteFormat.png);
    if (bytes == null) return;
    final support = await getApplicationSupportDirectory();
    await File('${support.path}/spike-${mode.name}.png')
        .writeAsBytes(bytes.buffer.asUint8List());
  }

  Future<void> _run() async {
    SchedulerBinding.instance.addTimingsCallback(_onTimings);
    final results = <Map<String, Object>>[];
    for (final mode in SpikeMode.values) {
      results.add(await _measure(mode));
    }
    SchedulerBinding.instance.removeTimingsCallback(_onTimings);
    final view = PlatformDispatcher.instance.views.first;
    final report = {
      'ticket': 'T-0005',
      'referenceDevice': false,
      'buildMode': kProfileMode
          ? 'profile'
          : (kReleaseMode ? 'release' : 'debug'),
      'os': Platform.operatingSystemVersion,
      'processors': Platform.numberOfProcessors,
      'render': '${_renderWidth}x$_renderHeight',
      'displaySize': '${view.physicalSize.width}x${view.physicalSize.height}',
      'refreshRate': view.display.refreshRate,
      'results': results,
    };
    final support = await getApplicationSupportDirectory();
    final file = File('${support.path}/spike-report.json');
    await file.writeAsString(
      const JsonEncoder.withIndent('  ').convert(report),
    );
    debugPrint('SPIKE_REPORT ${file.path}');
    if (widget.exitWhenDone) exit(0);
    if (mounted) setState(() => _reportPath = file.path);
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final mode = _mode;
    final modeName = switch (mode) {
      SpikeMode.zeroCopy => l10n.spikeZeroCopy,
      SpikeMode.pixelCopy => l10n.spikePixelCopy,
      null => '',
    };
    final id = _textureId;
    final path = _reportPath;
    return Scaffold(
      appBar: AppBar(title: Text(l10n.spikeTitle)),
      body: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Text(
              path != null
                  ? l10n.spikeReportSaved(path)
                  : l10n.spikeMeasuring(modeName),
            ),
          ),
          Expanded(
            child: Center(
              child: AspectRatio(
                aspectRatio: _renderWidth / _renderHeight,
                child: LayoutBuilder(
                  builder: (context, constraints) => RepaintBoundary(
                    key: _preview,
                    child: Stack(
                      children: [
                        if (id != null)
                          Positioned.fill(child: Texture(textureId: id)),
                        ValueListenableBuilder(
                          valueListenable: _chipOffset,
                          builder: (context, offset, child) => Positioned(
                            left:
                                (offset.dx + 1) /
                                2 *
                                (constraints.maxWidth - _chipSize),
                            top:
                                (offset.dy + 1) /
                                2 *
                                (constraints.maxHeight - _chipSize),
                            child: child!,
                          ),
                          child: const _Chip(),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _Chip extends StatelessWidget {
  const _Chip();

  @override
  Widget build(BuildContext context) {
    return Container(
      width: _chipSize,
      height: _chipSize,
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.primary,
        borderRadius: BorderRadius.circular(16),
        boxShadow: const [BoxShadow(blurRadius: 12)],
      ),
    );
  }
}
