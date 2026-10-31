import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../data/vault.dart';
import '../../data/views.dart';
import 'vault_bloc.dart';
import 'vault_page.dart';

/// A new item in the shown scope, or [item] changed (ui §4): the type, name,
/// key (from the name until it is typed), value and note. A secret's value
/// is never filled in: left empty, it stays. The type changes only between
/// text and url (B7).
class VaultEditorPage extends StatefulWidget {
  const VaultEditorPage({super.key, this.item});

  final VaultItemView? item;

  @override
  State<VaultEditorPage> createState() => _VaultEditorPageState();
}

class _VaultEditorPageState extends State<VaultEditorPage> {
  late VaultKind _kind = widget.item?.kind ?? VaultKind.text;
  late final _name = TextEditingController(text: widget.item?.name ?? '');
  late final _key = TextEditingController(text: widget.item?.envName ?? '');
  // A secret's value is never sent back to the editor; a text or url is.
  late final _value = TextEditingController(text: widget.item?.kind == VaultKind.secret ? '' : widget.item?.value ?? '');
  late final _note = TextEditingController(text: widget.item?.description ?? '');

  /// The key follows the name until it is typed (a new item only).
  late bool _keyTyped = widget.item != null;
  bool _showSecret = false;
  bool _saving = false;
  bool _tried = false;
  String? _error;

  bool get _editing => widget.item != null;

  @override
  void dispose() {
    _value.clear();
    for (final c in [_name, _key, _value, _note]) {
      c.dispose();
    }
    super.dispose();
  }

  /// The keys of the other items in the item's scope.
  Iterable<String> _taken(VaultState state) {
    final item = widget.item;
    final scope = item == null || item.projectId == state.projectId ? state.items : state.companyItems;
    return scope.where((i) => i.id != item?.id).map((i) => i.envName);
  }

  String? _nameError() => _name.text.trim().isEmpty ? 'Enter a name.' : null;
  String? _keyError(VaultState state) => envNameError(_key.text.trim(), _taken(state));
  String? _valueError() =>
      vaultValueError(_kind, _kind == VaultKind.text ? _value.text : _value.text.trim(), keep: _editing && (widget.item!.kind == VaultKind.secret || widget.item!.undecryptable == true));

  void _failed(String reason) {
    if (!mounted) return;
    setState(() {
      _saving = false;
      _error = reason;
    });
  }

  Future<void> _save() async {
    final bloc = context.read<VaultBloc>();
    setState(() => _tried = true);
    if (_nameError() != null || _keyError(bloc.state) != null || _valueError() != null) return;
    final name = _name.text.trim();
    final key = _key.text.trim();
    final note = _note.text.trim();
    final value = _kind == VaultKind.text ? _value.text : _value.text.trim();
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final item = widget.item;
      if (item == null) {
        await bloc.create(VaultCreateParams(
          description: note.isEmpty ? null : note,
          envName: key,
          kind: _kind,
          name: name,
          projectId: bloc.state.projectId,
          value: value,
        ));
      } else {
        final params = VaultUpdateParams(
          itemId: item.id,
          name: name == item.name ? null : name,
          envName: key == item.envName ? null : key,
          // "" clears the note.
          description: note == (item.description ?? '') ? null : note,
          kind: _kind == item.kind ? null : _kind,
          value: item.kind == VaultKind.secret ? (value.isEmpty ? null : value) : (value == (item.value ?? '') ? null : value),
        );
        final unchanged = params.name == null && params.envName == null && params.description == null && params.kind == null && params.value == null;
        if (!unchanged) await bloc.update(item, params);
      }
      _value.clear();
      if (mounted) Navigator.of(context).pop();
    } on ProtocolException catch (e) {
      _failed(saveFailure(e));
    } on NotSentException {
      _failed('Not sent: not connected to the PC.');
    } on DisconnectedException catch (e) {
      _failed(e.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final state = context.watch<VaultBloc>().state;
    final item = widget.item;
    final choices = kindChoices(item?.kind);
    final keyChanged = item != null && _key.text.trim() != item.envName;
    final scope = item == null ? scopeName(context, state.projectId) : (item.projectId == null ? 'Company' : scopeName(context, item.projectId));
    InputDecoration field(String label, {String? hint, String? error, String? helper, Widget? suffix}) =>
        InputDecoration(labelText: label, hintText: hint, errorText: _tried ? error : null, helperText: helper, helperMaxLines: 2, suffixIcon: suffix, border: InputBorder.none);
    return Scaffold(
      backgroundColor: Tokens.bg,
      appBar: GlassAppBar(
        title: Text(item == null ? 'New item · $scope' : 'Edit ${item.name}'),
        leading: IconButton(tooltip: 'Cancel', icon: const Icon(CupertinoIcons.xmark), onPressed: _saving ? null : () => Navigator.of(context).maybePop()),
        actions: [
          TextButton(key: const ValueKey('vault-save'), onPressed: _saving ? null : _save, child: Text('Save', style: TextStyle(fontWeight: FontWeight.w600, color: Tokens.accent))),
        ],
      ),
      body: ListView(
        padding: EdgeInsets.only(top: 8, bottom: listBottom(context)),
        children: [
          if (_error != null)
            Padding(
              key: const ValueKey('vault-editor-error'),
              padding: const EdgeInsets.fromLTRB(32, 0, 32, 8),
              child: Text(_error!, style: TextStyle(fontSize: 15, color: Tokens.negative)),
            ),
          const GroupHeader('Type'),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: choices.length == 1
                ? Text('Secret · it stays a secret. For another type, make a new item.', key: const ValueKey('vault-kind-locked'), style: TextStyle(fontSize: 15, color: Tokens.label2))
                : PillSegments<VaultKind>(
                    key: const ValueKey('vault-kind'),
                    segments: [for (final k in choices) (k, kindLabel(k))],
                    selected: _kind,
                    onChanged: (k) => setState(() {
                      // What was typed as a secret never shows in clear text (M-m6).
                      if (_kind == VaultKind.secret && k != VaultKind.secret) _value.clear();
                      _kind = k;
                      _showSecret = false;
                    }),
                  ),
          ),
          if (item != null && item.kind != VaultKind.secret)
            Padding(
              padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
              child: Text('Text and URL can change into each other; a secret cannot. For a secret, make a new item.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
            ),
          const GroupHeader('Item'),
          InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16), children: [
            TextField(
              key: const ValueKey('vault-name'),
              controller: _name,
              textCapitalization: TextCapitalization.sentences,
              decoration: field('Name', error: _nameError()),
              onChanged: (v) => setState(() {
                if (!_keyTyped) _key.text = envNameFrom(v);
              }),
            ),
            TextField(
              key: const ValueKey('vault-env'),
              controller: _key,
              autocorrect: false,
              enableSuggestions: false,
              textCapitalization: TextCapitalization.characters,
              style: TextStyle(fontFamily: Tokens.mono),
              decoration: field('Key', error: _keyError(state), helper: keyChanged ? envRenameWarning(item.envName) : 'A–Z, 0–9 and _ · unique in this scope'),
              onChanged: (_) => setState(() => _keyTyped = true),
            ),
            if (_kind == VaultKind.secret)
              TextField(
                key: const ValueKey('vault-value'),
                controller: _value,
                obscureText: !_showSecret,
                enableSuggestions: false,
                autocorrect: false,
                keyboardType: TextInputType.visiblePassword,
                decoration: field(
                  item == null ? 'Value' : 'New value',
                  hint: item == null ? null : 'Leave empty to keep the current secret',
                  error: _valueError(),
                  // Only while typing, on this phone: not saved yet, so not logged.
                  suffix: IconButton(
                    tooltip: _showSecret ? 'Hide what you typed' : 'Show what you typed',
                    icon: Icon(_showSecret ? CupertinoIcons.eye_slash : CupertinoIcons.eye),
                    onPressed: () => setState(() => _showSecret = !_showSecret),
                  ),
                ),
                onChanged: (_) => setState(() {}),
              )
            else
              TextField(
                key: const ValueKey('vault-value'),
                controller: _value,
                minLines: _kind == VaultKind.text ? 2 : 1,
                maxLines: _kind == VaultKind.text ? 8 : 1,
                autocorrect: _kind == VaultKind.text,
                keyboardType: _kind == VaultKind.url ? TextInputType.url : TextInputType.multiline,
                decoration: field('Value', hint: _kind == VaultKind.url ? 'https://…' : null, error: _valueError(), helper: vaultNotSecretHint),
                onChanged: (_) => setState(() {}),
              ),
            TextField(
              key: const ValueKey('vault-note'),
              controller: _note,
              minLines: 1,
              maxLines: 4,
              textCapitalization: TextCapitalization.sentences,
              decoration: field('Note (optional)', helper: 'Employees see this. Never a secret.'),
            ),
          ]),
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
            child: Text('Scope: $scope', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 6, 32, 0),
            child: Text(vaultReadersNotice, style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ),
        ],
      ),
    );
  }
}

/// What to say when the PC refuses a save.
String saveFailure(ProtocolException e) => switch (e.code) {
      ErrorCode.vaultKindLocked => 'A secret stays a secret, and nothing becomes one. Make a new item instead.',
      ErrorCode.versionConflict => 'This item was changed elsewhere meanwhile. Open it again.',
      _ => e.shown,
    };
