import 'package:shorted/features/preview/preview_controller.dart';

class FakeTransport implements PreviewTransport {
  final seeks = <int>[];

  @override
  int positionUs = 0;

  @override
  bool isPlaying = false;

  @override
  Future<int?> startTexture() async => 1;

  @override
  Future<void> stopTexture() async {}

  @override
  void seek(int timeUs) => seeks.add(timeUs);

  @override
  void play() => isPlaying = true;

  @override
  void pause() => isPlaying = false;
}
