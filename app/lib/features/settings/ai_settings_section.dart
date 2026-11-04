import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../bridge/api/ai.dart';
import '../../bridge/error.dart';
import '../../design_system/components.dart';
import '../../design_system/tokens.g.dart';
import '../../l10n/generated/app_localizations.dart';
import '../project/project_dialogs.dart';
import '../project/project_providers.dart';

String providerName(AiProviderView provider) => switch (provider) {
  AiProviderView.openAi => 'OpenAI',
  AiProviderView.anthropic => 'Anthropic',
  AiProviderView.gemini => 'Google Gemini',
  AiProviderView.openAiCompatible => 'OpenAI-compatible',
};

/// What a failed connection test means, in words the user can act on (T-0805).
String connectionError(AppLocalizations l10n, Object error) {
  if (error is EngineFailure) {
    switch (error.code) {
      case ErrorCode.aiKeyRejected:
        return l10n.aiKeyRejected;
      case ErrorCode.aiRateLimited:
        return l10n.aiRateLimited;
      case ErrorCode.aiUnavailable:
        return l10n.aiUnavailable;
      case ErrorCode.aiUnreachable:
        return l10n.aiUnreachable;
      case ErrorCode.aiNotConfigured:
        return l10n.aiNotConfigured;
      case ErrorCode.keyStorage:
        return l10n.aiKeyStorage;
      default:
        break;
    }
  }
  return errorMessage(l10n, error);
}

/// S60 AI providers: the provider, its model, a key kept in the system keychain, and a test.
/// The key field is write-only: a stored key is never shown again.
class AiSettingsSection extends ConsumerStatefulWidget {
  const AiSettingsSection({super.key});

  @override
  ConsumerState<AiSettingsSection> createState() => _AiSettingsSectionState();
}

class _AiSettingsSectionState extends ConsumerState<AiSettingsSection> {
  AiSettingsView? _settings;
  AiProviderView _provider = AiProviderView.anthropic;
  final _model = TextEditingController();
  final _baseUrl = TextEditingController();
  final _key = TextEditingController();
  String? _result;
  bool _resultOk = false;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _model.dispose();
    _baseUrl.dispose();
    _key.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final settings = await ref.read(projectGatewayProvider).aiSettings();
    if (!mounted) return;
    setState(() {
      _settings = settings;
      _provider = settings.provider ?? AiProviderView.anthropic;
      _model.text = settings.model;
      _baseUrl.text = settings.baseUrl;
    });
  }

  Future<void> _act(Future<void> Function() action) async {
    setState(() {
      _busy = true;
      _result = null;
    });
    try {
      await action();
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _save() => _act(() async {
    final l10n = AppLocalizations.of(context);
    final key = _key.text.trim();
    try {
      final saved = await ref
          .read(projectGatewayProvider)
          .saveAiSettings(
            _provider,
            model: _model.text,
            baseUrl: _baseUrl.text,
            key: key.isEmpty ? null : key,
          );
      _key.clear();
      if (!mounted) return;
      setState(() {
        _settings = saved;
        _result = l10n.aiSaved;
        _resultOk = true;
      });
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _result = connectionError(l10n, error);
          _resultOk = false;
        });
      }
    }
  });

  Future<void> _test() => _act(() async {
    final l10n = AppLocalizations.of(context);
    try {
      await ref.read(projectGatewayProvider).testAiConnection();
      if (mounted) {
        setState(() {
          _result = l10n.aiConnected;
          _resultOk = true;
        });
      }
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _result = connectionError(l10n, error);
          _resultOk = false;
        });
      }
    }
  });

  Future<void> _forget() => _act(() async {
    final settings = await ref.read(projectGatewayProvider).forgetAiKey();
    if (mounted) setState(() => _settings = settings);
  });

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final settings = _settings;
    if (settings == null) return const LinearProgressIndicator();
    final gateway = ref.read(projectGatewayProvider);
    final hasKey = settings.hasKey && settings.provider == _provider;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          l10n.aiExplained,
          style: const TextStyle(color: ShortedColors.textMuted),
        ),
        const SizedBox(height: ShortedSpace.s),
        Wrap(
          spacing: ShortedSpace.s,
          runSpacing: ShortedSpace.s,
          children: [
            for (final provider in AiProviderView.values)
              ShortedChip(
                label: providerName(provider),
                selected: _provider == provider,
                onTap: () => setState(() => _provider = provider),
              ),
          ],
        ),
        const SizedBox(height: ShortedSpace.s),
        TextField(
          key: const ValueKey('ai-model'),
          controller: _model,
          decoration: InputDecoration(
            labelText: l10n.aiModel,
            hintText: gateway.defaultAiModel(_provider),
          ),
        ),
        if (_provider == AiProviderView.openAiCompatible)
          TextField(
            key: const ValueKey('ai-base-url'),
            controller: _baseUrl,
            keyboardType: TextInputType.url,
            decoration: InputDecoration(
              labelText: l10n.aiBaseUrl,
              hintText: 'http://localhost:11434',
            ),
          ),
        TextField(
          key: const ValueKey('ai-key'),
          controller: _key,
          obscureText: true,
          autocorrect: false,
          enableSuggestions: false,
          decoration: InputDecoration(
            labelText: l10n.aiKey,
            helperText: hasKey ? l10n.aiKeyStored : l10n.aiKeyHelp,
          ),
        ),
        const SizedBox(height: ShortedSpace.s),
        Row(
          children: [
            Expanded(
              child: PrimaryButton(
                label: l10n.save,
                onPressed: _busy ? null : _save,
              ),
            ),
            const SizedBox(width: ShortedSpace.s),
            Expanded(
              child: SecondaryButton(
                label: l10n.aiTest,
                onPressed: _busy || !hasKey ? null : _test,
              ),
            ),
          ],
        ),
        if (hasKey)
          TextButton(
            onPressed: _busy ? null : _forget,
            child: Text(l10n.aiForgetKey),
          ),
        if (_result case final result?)
          Padding(
            padding: const EdgeInsets.only(top: ShortedSpace.s),
            child: Text(
              result,
              style: TextStyle(
                color: _resultOk ? ShortedColors.accent : ShortedColors.danger,
              ),
            ),
          ),
      ],
    );
  }
}
