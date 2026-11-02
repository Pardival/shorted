import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'app/shorted_app.dart';
import 'bridge/api.dart' as engine;
import 'bridge/error.dart';
import 'bridge/frb_generated.dart';
import 'features/preview/preview_controller.dart';
import 'features/project/project_gateway.dart';
import 'features/project/project_providers.dart';
import 'features/settings/app_settings.dart';
import 'features/spike/spike_screen.dart';

const _runSpike = bool.fromEnvironment('SHORTED_SPIKE');

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  await _startLogging();
  final support = await getApplicationSupportDirectory();
  final settings = File('${support.path}/settings.json');
  final locale = await loadLocale(settings);
  runApp(
    ProviderScope(
      overrides: [
        settingsFileProvider.overrideWithValue(settings),
        savedLocaleProvider.overrideWithValue(locale),
        projectGatewayProvider.overrideWithValue(EngineGateway(support.path)),
        previewTransportProvider.overrideWithValue(EngineTransport()),
      ],
      child: ShortedApp(
        engineVersion: engine.engineVersion(),
        measurementScreen: _runSpike
            ? SpikeScreen(exitWhenDone: Platform.isMacOS)
            : null,
      ),
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
