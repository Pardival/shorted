import 'package:flutter/material.dart';

import 'app/shorted_app.dart';
import 'bridge/api.dart' as engine;
import 'bridge/frb_generated.dart';

const _counterIntervalMs = 1000;

Future<void> main() async {
  await RustLib.init();
  runApp(
    ShortedApp(
      engineVersion: engine.engineVersion(),
      counter: engine
          .counterStream(intervalMs: _counterIntervalMs)
          .map((value) => value.toInt()),
    ),
  );
}
