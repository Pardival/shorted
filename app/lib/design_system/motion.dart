import 'package:flutter/material.dart';

import 'tokens.g.dart';

/// A motion token, shortened when the system asks for less motion.
Duration motionDuration(BuildContext context, Duration duration) =>
    MediaQuery.disableAnimationsOf(context)
    ? ShortedMotion.reducedMotion
    : duration;

const _fadeOutShare = 0.35;
const _sharedAxisShift = 0.04;

/// Shared axis X for sections: content arrives sideways as it fades in. With reduced motion it is
/// a plain fade, without movement.
Widget fadeThrough(Widget child, Animation<double> animation) {
  return Builder(
    builder: (context) {
      final still = MediaQuery.disableAnimationsOf(context);
      final fade = CurvedAnimation(
        parent: animation,
        curve: const Interval(_fadeOutShare, 1, curve: Curves.easeOut),
      );
      if (still) return FadeTransition(opacity: animation, child: child);
      return FadeTransition(
        opacity: fade,
        child: SlideTransition(
          position: Tween(
            begin: const Offset(_sharedAxisShift, 0),
            end: Offset.zero,
          ).animate(fade),
          child: child,
        ),
      );
    },
  );
}

const _sharedAxisZoom = 0.96;

/// Shared axis Z for screens: scale and fade. With reduced motion it is a plain fade.
Widget screenTransition(Widget child, Animation<double> animation) {
  return Builder(
    builder: (context) {
      if (MediaQuery.disableAnimationsOf(context)) {
        return FadeTransition(opacity: animation, child: child);
      }
      final eased = CurvedAnimation(
        parent: animation,
        curve: Curves.easeOutCubic,
      );
      return FadeTransition(
        opacity: eased,
        child: ScaleTransition(
          scale: Tween(begin: _sharedAxisZoom, end: 1.0).animate(eased),
          child: child,
        ),
      );
    },
  );
}
