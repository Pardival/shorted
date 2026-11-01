import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:shorted/bridge/api.dart' as engine;
import 'package:shorted/bridge/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(RustLib.init);

  test('sync call returns the engine version', () {
    expect(engine.engineVersion(), isNotEmpty);
  });

  test('async call runs in the engine', () async {
    final sum = await engine.add(left: 2, right: 3);
    expect(sum.toInt(), 5);
  });

  test('stream delivers consecutive values from Rust', () async {
    final values = await engine
        .counterStream(intervalMs: 5)
        .take(3)
        .map((value) => value.toInt())
        .toList();
    expect(values, [0, 1, 2]);
  });
}
