import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/vault.dart';
import '../../data/views.dart';
import '../links/external_link.dart' show launchExternal;
import 'reveal_bloc.dart';
import 'vault_bloc.dart';
import 'vault_page.dart';

/// One item: its value (a secret masked, with Show and Copy), key, note,
/// who changed it last, and its history from the audit log.
class VaultItemPage extends StatelessWidget {
  const VaultItemPage({super.key, required this.itemId});

  final String itemId;

  @override
  Widget build(BuildContext context) => BlocProvider(
        create: (context) => RevealBloc(context.read<AureliusRepository>()),
        child: _ItemView(itemId: itemId),
      );
}

class _ItemView extends StatefulWidget {
  const _ItemView({required this.itemId});

  final String itemId;

  @override
  State<_ItemView> createState() => _ItemViewState();
}

class _ItemViewState extends State<_ItemView> with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  /// Away from the app (another app, the lock screen): hidden at once (ui §3).
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) context.read<RevealBloc>().add(const RevealHidden());
  }

  Future<void> _delete(BuildContext context, VaultItemView item) async {
    final bloc = context.read<VaultBloc>();
    if (!await confirm(context, title: archiveQuestion(item), body: archiveDetail(item), action: 'Delete', danger: true)) return;
    try {
      await bloc.archive(item);
      if (context.mounted) Navigator.of(context).maybePop();
    } on ProtocolException catch (e) {
      if (context.mounted) toast(context, e.shown);
    } on NotSentException catch (e) {
      if (context.mounted) toast(context, e.toString());
    } on DisconnectedException catch (e) {
      if (context.mounted) toast(context, e.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final item = context.select<VaultBloc, VaultItemView?>((b) => b.state.item(widget.itemId));
    if (item == null) {
      return const LargeTitleScaffold(title: 'Item', fill: EmptyState('This item is gone', detail: 'It was deleted, here or on another device.'));
    }
    final me = context.read<VaultBloc>().me;
    final updatedBy = item.updatedBy.id == me ? 'You' : item.updatedBy.username;
    return BlocListener<RevealBloc, RevealState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, s) {
        toast(context, s.notice!);
        context.read<RevealBloc>().add(const RevealNoticeShown());
      },
      child: LargeTitleScaffold(
        title: item.name,
        actions: [
          GlassButton(
            tooltip: 'Edit',
            icon: CupertinoIcons.pencil,
            onPressed: () {
              // The editor covers this screen: leaving it hides the value (ui §3, M-m7).
              context.read<RevealBloc>().add(const RevealHidden());
              openVaultEditor(context, item: item);
            },
          ),
          GlassButton(tooltip: 'Delete', icon: CupertinoIcons.trash, color: Tokens.negative, onPressed: () => _delete(context, item)),
        ],
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 4, 20, 0),
            child: Wrap(spacing: 8, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: [
              Text('${kindLabel(item.kind)} · ${item.projectId == null ? 'Company' : scopeName(context, item.projectId)}', style: TextStyle(fontSize: 15, color: Tokens.label2)),
              if (overridesCompany(item)) const OverridesPill(),
            ]),
          ),
          const GroupHeader('Value'),
          InsetGroup(children: [_Value(item: item)]),
          GroupHeader('Key', info: 'How a run gets it: the environment variable, and the name in the vault tool.'),
          InsetGroup(children: [
            GroupRow(
              key: const ValueKey('vault-item-key'),
              title: item.envName,
              titleStyle: TextStyle(fontSize: 16, fontFamily: Tokens.mono, color: Tokens.label),
              trailing: CopyButton(text: item.envName, tooltip: 'Copy key'),
            ),
          ]),
          if (item.description?.isNotEmpty == true) ...[
            const GroupHeader('Note'),
            InsetGroup(children: [Padding(padding: const EdgeInsets.all(16), child: Text(item.description!, style: TextStyle(fontSize: 16, color: Tokens.label)))]),
          ],
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 10, 32, 0),
            child: Text('Changed by $updatedBy · ${Fmt.ago(item.updatedAtMs)}', style: TextStyle(fontSize: 13, color: Tokens.label2)),
          ),
          const GroupHeader('History'),
          _History(itemId: item.id, version: item.version),
        ],
      ),
    );
  }
}

/// The value as its kind shows it: text in full, a url as a link, a secret
/// masked until Show (30 s), Copy without showing it.
class _Value extends StatelessWidget {
  const _Value({required this.item});

  final VaultItemView item;

  @override
  Widget build(BuildContext context) {
    if (item.undecryptable == true) {
      return Padding(
        key: const ValueKey('vault-undecryptable'),
        padding: const EdgeInsets.all(16),
        child: Text('Cannot be decrypted: the vault key on the PC does not fit this value.', style: TextStyle(fontSize: 15, color: Tokens.warning)),
      );
    }
    final value = item.value ?? '';
    return switch (item.kind) {
      VaultKind.text => Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 8, 12),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Expanded(child: SelectableText(value, key: const ValueKey('vault-value'), style: TextStyle(fontSize: 16, color: Tokens.label))),
            CopyButton(text: value),
          ]),
        ),
      VaultKind.url => GroupRow(
          key: const ValueKey('vault-value'),
          title: value,
          titleLines: 3,
          titleStyle: TextStyle(fontSize: 16, color: Tokens.accent, decoration: TextDecoration.underline, decorationColor: Tokens.accent),
          trailing: CopyButton(text: value),
          onTap: validVaultUrl(value) ? () => launchExternal(Uri.parse(value.trim())) : null,
        ),
      VaultKind.secret => _Secret(item: item),
    };
  }
}

class _Secret extends StatelessWidget {
  const _Secret({required this.item});

  final VaultItemView item;

  /// Show and Copy ask first (MU4-10 AC2); Cancel sends nothing, so no reveal
  /// and no audit row.
  Future<void> _confirmed(BuildContext context, {required String title, required String body, required String action, required RevealEvent event}) async {
    final bloc = context.read<RevealBloc>();
    if (await confirm(context, title: title, body: body, action: action) && !bloc.isClosed) bloc.add(event);
  }

  @override
  Widget build(BuildContext context) {
    final reveal = context.watch<RevealBloc>().state;
    final bloc = context.read<RevealBloc>();
    final value = reveal.itemId == item.id ? reveal.value : null;
    final left = reveal.secondsLeft;
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Semantics(
          label: value == null ? hiddenLabel(item) : null,
          excludeSemantics: value == null,
          // Not selectable: a copy goes only through Copy (sensitive, cleared after 60 s).
          child: Text(
            value ?? maskedValue,
            key: const ValueKey('vault-secret-value'),
            style: TextStyle(fontSize: 16, fontFamily: Tokens.mono, color: Tokens.label, letterSpacing: value == null ? 2 : 0),
          ),
        ),
        const SizedBox(height: 10),
        Wrap(spacing: 8, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: [
          // One button whose label changes, so the focus stays on it (ui §6, M-m4).
          value == null
              ? _Action(key: const ValueKey('vault-show'), label: 'Show', semantics: 'Show ${item.name}', icon: CupertinoIcons.eye, onTap: reveal.busy ? null : () => _confirmed(context, title: showQuestion(item), body: showDetail(), action: 'Show', event: RevealShown(item.id)))
              : _Action(key: const ValueKey('vault-show'), label: 'Hide', semantics: 'Hide ${item.name}', icon: CupertinoIcons.eye_slash, onTap: () => bloc.add(const RevealHidden())),
          _Action(key: const ValueKey('vault-copy'), label: 'Copy', semantics: 'Copy ${item.name}', icon: CupertinoIcons.doc_on_doc, onTap: reveal.busy ? null : () => _confirmed(context, title: copyQuestion(item), body: copyDetail(), action: 'Copy', event: RevealCopied(item.id))),
          if (value != null) Text('Hides in 0:${left.toString().padLeft(2, '0')}', key: const ValueKey('vault-countdown'), style: Tokens.monoSmall()),
        ]),
        const SizedBox(height: 6),
        // Read once on Show and once on Hide, not every second (ui §6); the hint apart, not announced.
        if (value == null) Text(showDetail(), key: const ValueKey('vault-show-hint'), style: TextStyle(fontSize: 12.5, color: Tokens.label2)),
        Semantics(
          key: const ValueKey('vault-reveal-live'),
          liveRegion: true,
          child: Text(
            value != null ? 'Shown for ${revealFor.inSeconds} seconds. This is logged.' : (reveal.hidden ? 'Hidden' : ''),
            style: TextStyle(fontSize: 12.5, color: Tokens.label2),
          ),
        ),
      ]),
    );
  }
}

class _Action extends StatelessWidget {
  const _Action({super.key, required this.label, required this.semantics, required this.icon, required this.onTap});

  final String label;
  final String semantics;
  final IconData icon;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: semantics,
        excludeSemantics: true,
        child: OutlinedButton.icon(onPressed: onTap, icon: Icon(icon, size: 17), label: Text(label)),
      );
}

/// The item's rows of the audit log, newest first (ui §5 History). Read
/// again after a change of the item and after each Show or Copy.
class _History extends StatefulWidget {
  const _History({required this.itemId, required this.version});

  final String itemId;
  final int version;

  @override
  State<_History> createState() => _HistoryState();
}

class _HistoryState extends State<_History> {
  List<VaultAuditRow>? _rows;
  String? _error;
  StreamSubscription<RevealState>? _reveals;

  @override
  void initState() {
    super.initState();
    _load();
    // Once per Show or Copy, when the PC's answer is in: not on each tick of the countdown (M-m2).
    _reveals = context.read<RevealBloc>().stream.distinct((a, b) => a.busy == b.busy).where((s) => !s.busy).listen((_) => _load());
  }

  @override
  void didUpdateWidget(_History old) {
    super.didUpdateWidget(old);
    if (old.version != widget.version) _load();
  }

  @override
  void dispose() {
    _reveals?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final page = await context.read<AureliusRepository>().vaultAuditLog(itemId: widget.itemId, limit: 20);
      if (mounted) {
        setState(() {
          _rows = page.rows;
          _error = null;
        });
      }
    } on ProtocolException catch (e) {
      if (mounted) setState(() => _error = e.shown);
    } on DisconnectedException catch (e) {
      if (mounted) setState(() => _error = e.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final rows = _rows;
    final me = context.read<VaultBloc>().me;
    if (rows == null) {
      return Padding(padding: const EdgeInsets.fromLTRB(32, 4, 32, 0), child: Text(_error ?? 'Loading…', style: TextStyle(fontSize: 15, color: Tokens.label2)));
    }
    if (rows.isEmpty) return Padding(padding: const EdgeInsets.fromLTRB(32, 4, 32, 0), child: Text('Nothing yet', style: TextStyle(fontSize: 15, color: Tokens.label2)));
    return InsetGroup(children: [for (final r in rows) AuditRowTile(row: r, me: me)]);
  }
}

/// "You revealed GitHub token", with the time and the device under it. Never a value.
class AuditRowTile extends StatelessWidget {
  const AuditRowTile({super.key, required this.row, this.me});

  final VaultAuditRow row;
  final String? me;

  @override
  Widget build(BuildContext context) => GroupRow(
        key: ValueKey('audit-${row.id}'),
        leading: row.actor is VaultActorBoardMember
            ? Monogram(auditWho(row.actor, me: me).characters.first.toUpperCase(), size: 30)
            : Container(
                width: 30,
                height: 30,
                alignment: Alignment.center,
                decoration: BoxDecoration(color: Tokens.fill, borderRadius: BorderRadius.circular(6)),
                child: Text(auditWho(row.actor).characters.first.toUpperCase(), style: TextStyle(fontWeight: FontWeight.w600, color: Tokens.label)),
              ),
        title: auditLine(row, me: me),
        titleLines: 3,
        titleStyle: TextStyle(fontSize: 15, color: Tokens.label),
        subtitle: [Fmt.ago(row.atMs), ?auditVia(row)].join(' · '),
      );
}
