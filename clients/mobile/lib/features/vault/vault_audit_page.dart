import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/vault.dart';
import '../../data/views.dart';
import 'vault_item_page.dart' show AuditRowTile;

/// The vault's audit log (ui §5): a scope's or every scope's, newest first,
/// "You revealed GitHub token" with the time and device. Read-only for
/// everyone, never a value. Who and action are filtered on the phone.
class VaultAuditLogPage extends StatefulWidget {
  const VaultAuditLogPage({super.key, this.projectId, required this.scopeName});

  /// The scope it opens on; null: the company.
  final String? projectId;
  final String scopeName;

  @override
  State<VaultAuditLogPage> createState() => _VaultAuditPageState();
}

class _VaultAuditPageState extends State<VaultAuditLogPage> {
  final List<VaultAuditRow> _rows = [];
  bool _more = false;
  bool _loading = true;
  String? _error;
  bool _all = false;
  VaultAuditAction? _action;
  AuditWho _who = AuditWho.anyone;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load({bool older = false}) async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final page = await context.read<AureliusRepository>().vaultAuditLog(
            projectId: _all ? null : widget.projectId,
            all: _all,
            before: older && _rows.isNotEmpty ? _rows.last.id : null,
          );
      if (!mounted) return;
      setState(() {
        if (!older) _rows.clear();
        _rows.addAll(page.rows);
        _more = page.more;
        _loading = false;
      });
    } on ProtocolException catch (e) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = e.shown;
        });
      }
    } on DisconnectedException catch (e) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = e.message;
        });
      }
    }
  }

  Future<void> _filters() async {
    await showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      builder: (sheet) => StatefulBuilder(
        builder: (sheet, setSheet) {
          void set(VoidCallback change) {
            setSheet(change);
            setState(() {});
          }

          return SafeArea(
            child: ListView(
              shrinkWrap: true,
              padding: const EdgeInsets.only(bottom: 12),
              children: [
                const GroupHeader('Scope', caps: true),
                RadioGroup<bool>(
                  groupValue: _all,
                  onChanged: (v) {
                    set(() => _all = v ?? false);
                    _load();
                  },
                  child: Column(children: [
                    RadioListTile<bool>(key: const ValueKey('audit-scope-this'), value: false, title: Text(widget.scopeName)),
                    const RadioListTile<bool>(key: ValueKey('audit-scope-all'), value: true, title: Text('All scopes')),
                  ]),
                ),
                const GroupHeader('Who', caps: true),
                RadioGroup<AuditWho>(
                  groupValue: _who,
                  onChanged: (v) => set(() => _who = v ?? AuditWho.anyone),
                  child: const Column(children: [
                    RadioListTile<AuditWho>(value: AuditWho.anyone, title: Text('Anyone')),
                    RadioListTile<AuditWho>(key: ValueKey('audit-who-board'), value: AuditWho.boardMembers, title: Text('Board members')),
                    RadioListTile<AuditWho>(key: ValueKey('audit-who-employees'), value: AuditWho.employees, title: Text('Employees (runs)')),
                  ]),
                ),
                const GroupHeader('Action', caps: true),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: Wrap(spacing: 8, runSpacing: 8, children: [
                    ChoiceChip(label: const Text('Any'), selected: _action == null, onSelected: (_) => set(() => _action = null)),
                    for (final a in VaultAuditAction.values)
                      ChoiceChip(key: ValueKey('audit-action-${a.wire}'), label: Text(auditActionLabel(a)), selected: _action == a, onSelected: (_) => set(() => _action = a)),
                  ]),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final me = context.read<AureliusRepository>().linkState.user?.id;
    final shown = filterAudit(_rows, action: _action, who: _who);
    final filtered = _action != null || _who != AuditWho.anyone;
    return LargeTitleScaffold(
      title: 'Audit log',
      actions: [GlassButton(tooltip: 'Filter', icon: CupertinoIcons.line_horizontal_3_decrease, onPressed: _filters)],
      onRefresh: () => _load(),
      fill: _rows.isEmpty && (_loading || _error != null)
          ? (_error != null ? ErrorRetry(_error!, onRetry: _load) : const Center(child: CircularProgressIndicator.adaptive()))
          : null,
      children: _rows.isEmpty && (_loading || _error != null)
          ? null
          : [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 4, 20, 0),
                child: Text(
                  [_all ? 'All scopes' : widget.scopeName, if (_who != AuditWho.anyone) _who == AuditWho.boardMembers ? 'Board members' : 'Employees', if (_action != null) auditActionLabel(_action!)].join(' · '),
                  key: const ValueKey('audit-scope'),
                  style: TextStyle(fontSize: 15, color: Tokens.label2),
                ),
              ),
              const SizedBox(height: 12),
              if (shown.isEmpty)
                Padding(
                  padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                  child: Text(filtered ? 'Nothing matches these filters here.' : 'Nothing logged yet.', style: TextStyle(fontSize: 15, color: Tokens.label2)),
                )
              else
                InsetGroup(dividerIndent: 60, children: [for (final r in shown) AuditRowTile(row: r, me: me)]),
              if (_more)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Center(
                    child: TextButton(key: const ValueKey('audit-older'), onPressed: _loading ? null : () => _load(older: true), child: const Text('Load older')),
                  ),
                ),
              Padding(
                padding: const EdgeInsets.fromLTRB(32, 16, 32, 0),
                child: Text('The audit log is kept for good and cannot be changed. It never holds a value.', style: TextStyle(fontSize: 13, color: Tokens.label2)),
              ),
            ],
    );
  }
}
