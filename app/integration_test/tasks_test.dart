import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shorted/bridge/api.dart' as engine;
import 'package:shorted/bridge/error.dart';
import 'package:shorted/bridge/frb_generated.dart';
import 'package:shorted/bridge/runtime.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(RustLib.init);

  test('a long task reports progress and can be cancelled from Dart', () async {
    final terminal = Completer<TaskEvent>();
    var progressEvents = 0;
    BigInt? taskId;
    engine.startDemoTask(durationMs: 60000).listen((event) {
      switch (event) {
        case TaskEvent_Started(taskId: final id):
          taskId = id;
        case TaskEvent_Progress():
          if (++progressEvents == 3) engine.cancelTask(taskId: taskId!);
        default:
          terminal.complete(event);
      }
    });

    final event = await terminal.future.timeout(const Duration(seconds: 10));
    expect(event, isA<TaskEvent_Cancelled>());
    expect(progressEvents, greaterThanOrEqualTo(3));
  });

  test('engine failures reach Dart as a safe code', () async {
    await expectLater(
      engine.initEngine(logDir: '/dev/null/logs'),
      throwsA(
        isA<EngineFailure>().having(
          (failure) => failure.code,
          'code',
          ErrorCode.internal,
        ),
      ),
    );
  });
}
