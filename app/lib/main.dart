import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

import 'app/shorted_app.dart';
import 'bridge/api.dart' as engine;
import 'bridge/error.dart';
import 'bridge/frb_generated.dart';
import 'features/spike/spike_screen.dart';

const _counterIntervalMs = 1000;
const _runSpike = bool.fromEnvironment('SHORTED_SPIKE');

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  await _startLogging();
  runApp(
    ShortedApp(
      engineVersion: engine.engineVersion(),
      counter: engine
          .counterStream(intervalMs: _counterIntervalMs)
          .map((value) => value.toInt()),
      home: _runSpike ? const SpikeScreen(exitWhenDone: true) : null,
    ),
  );
}

/// Logs are diagnostics only: failing to open them must not stop the app.
Future<void> _startLogging() async {
  final support = await getApplicationSupportDirectory();
  try {
    await engine.initEngine(logDir: '${support.path}/logs');
  } on EngineFailure catch (failure) {
    debugPrint('Engine logging unavailable: ${failure.code.name}');
  }
}
