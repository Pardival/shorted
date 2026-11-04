import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Where settings live. Null keeps them in memory, as in tests.
final settingsFileProvider = Provider<File?>((ref) => null);

/// The language saved last time, read before the app starts.
final savedLocaleProvider = Provider<Locale?>((ref) => null);

/// Whether onboarding was seen, read before the app starts. Tests start past it.
final savedOnboardedProvider = Provider<bool>((ref) => true);

/// The interface languages, by code, each in its own language.
const languageNames = {
  'en': 'English',
  'fr': 'Français',
  'es': 'Español',
  'pt': 'Português',
};

/// A settings file that cannot be read counts as no settings.
Future<Map<String, Object?>> loadSettings(File file) async {
  try {
    final saved = jsonDecode(await file.readAsString());
    return saved is Map<String, Object?> ? saved : {};
  } on Object {
    return {};
  }
}

Locale? savedLocale(Map<String, Object?> settings) {
  final code = settings['locale'];
  return code is String ? Locale(code) : null;
}

bool savedOnboarded(Map<String, Object?> settings) =>
    settings['onboarded'] == true;

/// Saves run one after another, so that two changes never overwrite each other.
Future<void> _saving = Future.value();

/// Changes one setting and keeps the others.
Future<void> _save(Ref ref, String key, Object? value) {
  final file = ref.read(settingsFileProvider);
  if (file == null) return Future.value();
  return _saving = _saving.then((_) async {
    final settings = await loadSettings(file);
    settings[key] = value;
    await file.writeAsString(jsonEncode(settings));
  });
}

/// The chosen language; null follows the system.
class LocaleSetting extends Notifier<Locale?> {
  @override
  Locale? build() => ref.read(savedLocaleProvider);

  Future<void> set(Locale? locale) async {
    state = locale;
    await _save(ref, 'locale', locale?.languageCode);
  }
}

final localeProvider = NotifierProvider<LocaleSetting, Locale?>(
  LocaleSetting.new,
);

/// Onboarding shows on the first launch only (FR-HOME-02).
class OnboardedSetting extends Notifier<bool> {
  @override
  bool build() => ref.read(savedOnboardedProvider);

  Future<void> finish() async {
    state = true;
    await _save(ref, 'onboarded', true);
  }
}

final onboardedProvider = NotifierProvider<OnboardedSetting, bool>(
  OnboardedSetting.new,
);
