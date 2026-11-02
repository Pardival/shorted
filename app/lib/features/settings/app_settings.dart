import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Where settings live. Null keeps them in memory, as in tests.
final settingsFileProvider = Provider<File?>((ref) => null);

/// The language saved last time, read before the app starts.
final savedLocaleProvider = Provider<Locale?>((ref) => null);

/// A settings file that cannot be read counts as no settings.
Future<Locale?> loadLocale(File file) async {
  try {
    final saved = jsonDecode(await file.readAsString());
    final code = saved is Map ? saved['locale'] : null;
    return code is String ? Locale(code) : null;
  } on Object {
    return null;
  }
}

/// The chosen language; null follows the system.
class LocaleSetting extends Notifier<Locale?> {
  @override
  Locale? build() => ref.read(savedLocaleProvider);

  Future<void> set(Locale? locale) async {
    state = locale;
    await ref
        .read(settingsFileProvider)
        ?.writeAsString(jsonEncode({'locale': locale?.languageCode}));
  }
}

final localeProvider = NotifierProvider<LocaleSetting, Locale?>(
  LocaleSetting.new,
);
