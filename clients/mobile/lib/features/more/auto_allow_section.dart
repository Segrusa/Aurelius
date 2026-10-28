import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

const autoAllowNeedsApprove = 'Turning auto-allow on or off needs the Approve right on this phone.';

/// Settings: let the safe read-only tools pass without a question (Approve
/// right). Hidden on servers without the `auto_allow` feature.
class AutoAllowSection extends StatefulWidget {
  const AutoAllowSection({super.key});

  @override
  State<AutoAllowSection> createState() => _AutoAllowSectionState();
}

class _AutoAllowSectionState extends State<AutoAllowSection> {
  late final AureliusRepository _repository = context.read<AureliusRepository>();
  AutoAllowSetting? _setting;
  bool _busy = false;

  bool get _canApprove => _repository.linkState.can('approve');

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    if (!_repository.linkState.has('auto_allow')) return;
    try {
      final setting = await _repository.autoAllowSetting(const Duration(seconds: 10));
      if (mounted) setState(() => _setting = setting);
    } on Object {
      // No answer: the section stays hidden.
    }
  }

  Future<void> _set(bool on) async {
    setState(() => _busy = true);
    try {
      final setting = await _repository.setAutoAllowEnabled(on);
      if (mounted) setState(() => _setting = setting);
    } on ProtocolException catch (e) {
      if (mounted) toast(context, e.isForbidden ? autoAllowNeedsApprove : (e.message.isEmpty ? 'Could not change auto-allow.' : e.message));
    } on Object {
      if (mounted) toast(context, 'Could not change auto-allow.');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final setting = _setting;
    if (setting == null) return const SizedBox.shrink();
    final muted = TextStyle(color: Tokens.label3, fontSize: 13);
    return Column(
      key: const ValueKey('auto-allow-section'),
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const GroupHeader('Permissions', caps: true),
        InsetGroup(children: [
          GroupRow(
            key: const ValueKey('auto-allow-toggle'),
            title: 'Auto-allow safe tools',
            subtitle: setting.tools.isEmpty ? null : setting.tools.join(', '),
            trailing: CupertinoSwitch(
              value: setting.enabled,
              activeTrackColor: Tokens.accent,
              onChanged: _canApprove && !_busy ? _set : null,
            ),
          ),
        ]),
        Padding(
          padding: const EdgeInsets.fromLTRB(32, 6, 32, 8),
          child: Text(
            'These read-only tools run without asking, in every mode except Plan. '
            'WebFetch and screenshots still ask. Applies from the next run.',
            key: const Key('auto-allow-explain'),
            style: muted,
          ),
        ),
        if (!_canApprove) Padding(padding: const EdgeInsets.fromLTRB(32, 0, 32, 8), child: Text(autoAllowNeedsApprove, style: muted)),
      ],
    );
  }
}
