import 'package:flutter_test/flutter_test.dart';
import 'package:shorted/features/spike/frame_stats.dart';

void main() {
  test('summarises durations', () {
    final stats = DurationStats.of(List.generate(101, (i) => i.toDouble()));
    expect(stats.count, 101);
    expect(stats.mean, 50);
    expect(stats.p50, 50);
    expect(stats.p95, 95);
    expect(stats.p99, 99);
    expect(stats.max, 100);
  });

  test('empty input gives zeros', () {
    expect(DurationStats.of([]).toJson()['max'], 0);
  });
}
