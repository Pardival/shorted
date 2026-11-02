import 'dart:async';

import 'package:flutter/foundation.dart';

/// Values per second in a waveform, as the engine makes them.
const peaksPerSecond = 100;

/// The waveform of each media, filled in as the engine delivers them. Listeners repaint.
class Waveforms extends ChangeNotifier {
  Waveforms(this._load);

  final Future<Uint8List> Function(String mediaId) _load;
  final _peaks = <String, Uint8List>{};
  final _requested = <String>{};
  var _disposed = false;

  /// Starts loading, once per media.
  void request(String mediaId) {
    if (!_requested.add(mediaId)) return;
    unawaited(_fetch(mediaId));
  }

  Future<void> _fetch(String mediaId) async {
    try {
      final peaks = await _load(mediaId);
      if (_disposed) return;
      _peaks[mediaId] = peaks;
      notifyListeners();
    } on Object {
      // A waveform is a nicety: without one the item keeps its plain colour.
    }
  }

  /// Loudest value from 0 to 1 between two times, or null before it has loaded.
  double? level(String mediaId, int fromUs, int toUs) {
    final peaks = _peaks[mediaId];
    if (peaks == null || peaks.isEmpty) return null;
    final first = (fromUs * peaksPerSecond / 1e6).floor().clamp(
      0,
      peaks.length,
    );
    final last = (toUs * peaksPerSecond / 1e6).ceil().clamp(
      first,
      peaks.length,
    );
    var loudest = 0;
    for (var index = first; index < last; index++) {
      if (peaks[index] > loudest) loudest = peaks[index];
    }
    return loudest / 255;
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
