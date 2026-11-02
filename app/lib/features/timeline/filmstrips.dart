import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';

import '../../bridge/api/media.dart';

class FilmstripFrame {
  const FilmstripFrame(this.timeUs, this.image);

  final int timeUs;
  final ui.Image image;
}

/// The thumbnails of each media, filled in as the engine delivers them. Listeners repaint.
class Filmstrips extends ChangeNotifier {
  Filmstrips(this._load);

  final Stream<ThumbnailView> Function(String mediaId) _load;
  final _strips = <String, List<FilmstripFrame>>{};
  final _subscriptions = <StreamSubscription<void>>[];
  var _disposed = false;

  /// Starts loading, once per media.
  void request(String mediaId) {
    if (_strips.containsKey(mediaId)) return;
    final strip = _strips[mediaId] = [];
    _subscriptions.add(
      _load(mediaId).asyncMap(_decode).listen(
        (frame) {
          if (_disposed) {
            frame.image.dispose();
            return;
          }
          strip.add(frame);
          notifyListeners();
        },
        // A strip is a nicety: without one the clip keeps its plain colour.
        onError: (Object _) {},
      ),
    );
  }

  static Future<FilmstripFrame> _decode(ThumbnailView view) {
    final done = Completer<FilmstripFrame>();
    ui.decodeImageFromPixels(
      view.rgba,
      view.width,
      view.height,
      ui.PixelFormat.rgba8888,
      (image) => done.complete(FilmstripFrame(view.timeUs.toInt(), image)),
    );
    return done.future;
  }

  /// The last thumbnail at or before `timeUs`, or the first one.
  FilmstripFrame? at(String mediaId, int timeUs) {
    final strip = _strips[mediaId];
    if (strip == null || strip.isEmpty) return null;
    var low = 0;
    var high = strip.length - 1;
    while (low < high) {
      final middle = (low + high + 1) ~/ 2;
      if (strip[middle].timeUs <= timeUs) {
        low = middle;
      } else {
        high = middle - 1;
      }
    }
    return strip[low];
  }

  @override
  void dispose() {
    _disposed = true;
    for (final subscription in _subscriptions) {
      unawaited(subscription.cancel());
    }
    for (final strip in _strips.values) {
      for (final frame in strip) {
        frame.image.dispose();
      }
    }
    super.dispose();
  }
}
