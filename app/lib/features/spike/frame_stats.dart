/// Summary of a list of durations in milliseconds, for spike reports.
class DurationStats {
  const DurationStats({
    required this.count,
    required this.mean,
    required this.p50,
    required this.p95,
    required this.p99,
    required this.max,
  });

  factory DurationStats.of(Iterable<double> values) {
    final sorted = values.toList()..sort();
    if (sorted.isEmpty) {
      return const DurationStats(
        count: 0,
        mean: 0,
        p50: 0,
        p95: 0,
        p99: 0,
        max: 0,
      );
    }
    double percentile(double fraction) =>
        sorted[((sorted.length - 1) * fraction).round()];
    return DurationStats(
      count: sorted.length,
      mean: sorted.reduce((a, b) => a + b) / sorted.length,
      p50: percentile(0.5),
      p95: percentile(0.95),
      p99: percentile(0.99),
      max: sorted.last,
    );
  }

  final int count;
  final double mean;
  final double p50;
  final double p95;
  final double p99;
  final double max;

  Map<String, Object> toJson() => {
    'count': count,
    'mean': mean,
    'p50': p50,
    'p95': p95,
    'p99': p99,
    'max': max,
  };
}
