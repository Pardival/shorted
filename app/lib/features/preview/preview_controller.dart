import 'package:flutter/services.dart';

import '../../bridge/api/preview.dart' as engine;

/// Transport of the preview: thin, so that widget tests can replace it.
abstract interface class PreviewTransport {
  Future<int?> startTexture();
  Future<void> stopTexture();
  void seek(int timeUs);
  void play();
  void pause();
  int get positionUs;
  bool get isPlaying;
}

/// 1080p preview, as in NFR-PERF-03. Low-end phones get 720p with T-1103.
const _previewWidth = 1080;
const _previewHeight = 1920;

class EngineTransport implements PreviewTransport {
  static const _channel = MethodChannel('shorted/preview');

  @override
  Future<int?> startTexture() async {
    try {
      return await _channel.invokeMethod<int>('start', {
        'width': _previewWidth,
        'height': _previewHeight,
      });
    } on MissingPluginException {
      // No texture path on this platform yet: the transport still works.
      return null;
    } on PlatformException {
      return null;
    }
  }

  @override
  Future<void> stopTexture() async {
    try {
      await _channel.invokeMethod<void>('stop');
    } on MissingPluginException {
      return;
    }
  }

  @override
  void seek(int timeUs) => engine.previewSeek(timeUs: timeUs);

  @override
  void play() => engine.previewPlay();

  @override
  void pause() => engine.previewPause();

  @override
  int get positionUs => engine.previewPositionUs();

  @override
  bool get isPlaying => engine.previewIsPlaying();
}
