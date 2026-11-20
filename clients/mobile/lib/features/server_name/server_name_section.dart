import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';
import 'server_name_bloc.dart';

/// The server refuses longer names (A-05); the field counts up to it but lets
/// the server say why, so its text shows in the dialog.
const serverNameMax = 40;

/// Settings → the server's name (SN-01 AC1), with "Default: ‹computer›" when
/// a board member chose one, and Edit only when this phone may rename it
/// (SN-01 AC5). Nothing on a server without the feature `server_name`.
class ServerNameSection extends StatelessWidget {
  const ServerNameSection({super.key, required this.link});

  final LinkState link;

  @override
  Widget build(BuildContext context) {
    if (!link.has(serverNameFeature)) return const SizedBox.shrink();
    return BlocBuilder<ServerNameBloc, ServerNameState>(
      builder: (context, state) {
        final view = state.view;
        final chosen = view?.chosen;
        return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const GroupHeader('Server', caps: true),
          InsetGroup(
            dividerIndent: 60,
            children: [
              GroupRow(
                key: const ValueKey('server-name'),
                leading: const IconTile(CupertinoIcons.textformat),
                title: 'Server name',
                value: view?.name ?? context.read<AureliusRepository>().host.name,
                subtitle: state.error ?? (chosen != null ? 'Default: ${view!.computerName}' : null),
                trailing: state.error != null
                    ? TextButton(key: const ValueKey('server-name-retry'), onPressed: () => context.read<ServerNameBloc>().add(const ServerNameRefreshed()), child: const Text('Try again'))
                    : link.canRenameServer && view != null
                        ? TextButton(key: const ValueKey('server-name-edit'), onPressed: () => showServerNameDialog(context, view), child: const Text('Edit'))
                        : null,
              ),
            ],
          ),
        ]);
      },
    );
  }
}

/// Rename the server in a dialog; true when saved. The server's refusal
/// (too long, a line break, no right) stays in the dialog.
Future<bool> showServerNameDialog(BuildContext context, ServerNameView view) async =>
    await showDialog<bool>(
      context: context,
      builder: (_) => _ServerNameDialog(view: view),
    ) ??
    false;

class _ServerNameDialog extends StatefulWidget {
  const _ServerNameDialog({required this.view});

  final ServerNameView view;

  @override
  State<_ServerNameDialog> createState() => _ServerNameDialogState();
}

class _ServerNameDialogState extends State<_ServerNameDialog> {
  late final _field = TextEditingController(text: widget.view.chosen ?? '');
  bool _saving = false;
  String? _error;

  @override
  void dispose() {
    _field.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await context.read<AureliusRepository>().setServerName(_field.text);
      if (mounted) Navigator.pop(context, true);
    } catch (e) {
      // ProtocolException shows the server's text; the others say what happened to the link.
      if (mounted) setState(() => _error = e is ProtocolException ? e.shown : '$e');
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Server name'),
      content: SizedBox(
        width: double.maxFinite,
        child: TextField(
          key: const ValueKey('server-name-field'),
          controller: _field,
          autofocus: true,
          enabled: !_saving,
          maxLength: serverNameMax,
          maxLengthEnforcement: MaxLengthEnforcement.none,
          onChanged: (_) => setState(() => _error = null),
          onSubmitted: (_) => _saving ? null : _save(),
          decoration: InputDecoration(
            labelText: 'Name',
            hintText: widget.view.computerName,
            helperText: "Leave empty to use the computer's name (${widget.view.computerName}).",
            helperMaxLines: 2,
            errorText: _error,
            errorMaxLines: 3,
            errorStyle: TextStyle(color: Tokens.negative),
          ),
        ),
      ),
      actions: [
        TextButton(key: const ValueKey('server-name-cancel'), onPressed: _saving ? null : () => Navigator.pop(context, false), child: const Text('Cancel')),
        FilledButton(key: const ValueKey('server-name-save'), onPressed: _saving ? null : _save, child: const Text('Save')),
      ],
    );
  }
}
