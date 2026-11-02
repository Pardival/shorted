import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';

import '../../design_system/motion.dart';
import '../../design_system/tokens.g.dart';

const _cut = Duration(milliseconds: 600);
const _barAngle = -0.1;
const _barHeight = 3.0;

/// S01: a lime bar slices the name, then Home takes over.
class SplashScreen extends StatefulWidget {
  const SplashScreen({super.key});

  @override
  State<SplashScreen> createState() => _SplashScreenState();
}

class _SplashScreenState extends State<SplashScreen>
    with SingleTickerProviderStateMixin {
  late final AnimationController _bar = AnimationController(vsync: this);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_bar.isAnimating || _bar.isCompleted) return;
    _bar.duration = motionDuration(context, _cut);
    _bar.forward().whenComplete(() {
      if (mounted) context.go('/home');
    });
  }

  @override
  void dispose() {
    _bar.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: Stack(
          alignment: Alignment.centerLeft,
          children: [
            Text('Shorted', style: Theme.of(context).textTheme.displaySmall),
            Positioned.fill(
              child: Transform.rotate(
                angle: _barAngle,
                child: Center(
                  child: SizeTransition(
                    sizeFactor: _bar,
                    axis: Axis.horizontal,
                    alignment: AlignmentDirectional.centerStart,
                    child: const ColoredBox(
                      color: ShortedColors.accent,
                      child: SizedBox(
                        height: _barHeight,
                        width: double.infinity,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
