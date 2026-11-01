import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'ticket_actions.dart';

/// `/settings/board` (§12.1, MU1-06; §12.2 for the company): the columns (six,
/// seven with Testing) — rename (not empty, unique) and reorder, each with its
/// fixed role that never follows the name (AC4) and no way to remove it (AC2,
/// B3) — the company's default dispatcher and, with `board_testing`, its
/// tester (ticket #5). Every change is saved at once with the board's version,
/// so a change made elsewhere meanwhile is refused.
class BoardSettingsPage extends StatefulWidget {
  const BoardSettingsPage({super.key});

  @override
  State<BoardSettingsPage> createState() => _BoardSettingsPageState();
}

class _BoardSettingsPageState extends State<BoardSettingsPage> {
  List<BoardColumnView>? _columns;
  int _version = 0;
  String? _error;
  bool _saving = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final board = await context.read<AureliusRepository>().board();
      if (!mounted) return;
      setState(() {
        _columns = [for (final c in board.columns) c.column]..sort((a, b) => a.position.compareTo(b.position));
        _version = boardVersion(board);
        _error = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _error = ticketFailure(e));
    }
  }

  /// Save every column in [columns]' order and names.
  Future<void> _save(List<BoardColumnView> columns, {required String done}) async {
    final before = _columns;
    setState(() {
      _columns = columns;
      _saving = true;
    });
    final ok = await ticketWrite(
      context,
      boardColumnsUpdate(columns: [for (final c in columns) BoardColumnEdit(columnId: c.id, name: c.name)]),
      expectedVersion: _version,
      retry: false,
      done: done,
    );
    if (!mounted) return;
    setState(() => _saving = false);
    if (!ok) setState(() => _columns = before);
    // The new version (and a change made elsewhere) comes with the board.
    await _load();
  }

  Future<void> _rename(BoardColumnView column) async {
    final columns = _columns!;
    final name = await showDialog<String>(context: context, builder: (_) => _RenameDialog(column: column, others: [for (final c in columns) if (c.id != column.id) c.name]));
    if (name == null || name == column.name || !mounted) return;
    await _save([for (final c in columns) c.id == column.id ? BoardColumnView(id: c.id, kind: c.kind, name: name, position: c.position, version: c.version) : c], done: 'Renamed to $name.');
  }

  /// [to] is the index once the column is out of its old place.
  Future<void> _reorder(int from, int to) async {
    final columns = [..._columns!];
    if (from == to) return;
    columns.insert(to, columns.removeAt(from));
    await _save(columns, done: 'Column order saved.');
  }

  Future<void> _pickDispatcher(CompanyState company) async {
    final active = company.employees.where((e) => e.status == LifecycleStatus.active).toList()..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    final current = company.company?.dispatcherEmployeeId;
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Default dispatcher')),
            ListTile(
              key: const ValueKey('dispatcher-none'),
              title: const Text('No dispatcher'),
              subtitle: const Text('Started tickets wait in Todo until a board member chooses an owner.'),
              trailing: current == null ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
            for (final e in active)
              ListTile(key: ValueKey('dispatcher-${e.id}'), title: Text(e.name), subtitle: Text(e.role), trailing: current == e.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (e.id,))),
          ]),
        ),
      ),
    );
    if (picked == null || picked.$1 == current || !mounted) return;
    final name = employeeName(company, picked.$1);
    context.read<CompanyBloc>().send(
          companyUpdate(UpdateCompany(dispatcher: SetDispatcher(employeeId: picked.$1))),
          expectedVersion: company.company?.version,
          retry: false,
          done: name == null ? 'No dispatcher: tickets wait for a board member.' : '$name is the default dispatcher.',
        );
  }

  /// The tester (ticket #5): who runs the full test suite once for a card in
  /// Testing; None leaves the Testing step out. A refusal comes as the bloc's notice.
  Future<void> _pickTester(CompanyState company) async {
    final active = company.employees.where((e) => e.status == LifecycleStatus.active).toList()..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    final current = company.company?.testerEmployeeId;
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Tester')),
            ListTile(
              key: const ValueKey('tester-none'),
              title: const Text('None: no Testing step'),
              subtitle: const Text('Reported tickets go straight to Review.'),
              trailing: current == null ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
            for (final e in active)
              ListTile(key: ValueKey('tester-${e.id}'), title: Text(e.name), subtitle: Text(e.role), trailing: current == e.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (e.id,))),
          ]),
        ),
      ),
    );
    if (picked == null || picked.$1 == current || !mounted) return;
    final name = employeeName(company, picked.$1);
    context.read<CompanyBloc>().send(
          companyUpdate(UpdateCompany(tester: SetTester(employeeId: picked.$1))),
          expectedVersion: company.company?.version,
          retry: false,
          done: name == null ? 'No tester: reported tickets go straight to Review.' : '$name tests reported tickets before Review.',
        );
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final testing = context.select<ConnectionBloc, bool>((b) => b.state.has(boardTestingFeature));
    final columns = _columns;
    final tester = testerName(company);
    final testerArchived = company.company?.testerEmployeeId != null && company.employee(company.company!.testerEmployeeId!)?.status != LifecycleStatus.active;
    final dispatcher = employeeName(company, company.company?.dispatcherEmployeeId);
    final archived = company.company?.dispatcherEmployeeId != null && company.employee(company.company!.dispatcherEmployeeId!)?.status != LifecycleStatus.active;
    return BlocListener<CompanyBloc, CompanyState>(
      listenWhen: (a, b) => b.notice != null && a.notice != b.notice,
      listener: (context, s) => toast(context, s.notice!),
      child: Scaffold(
        appBar: const GlassAppBar(title: Text('Board')),
        body: columns == null
            ? (_error != null ? ErrorRetry(_error!, onRetry: _load) : const Center(child: CircularProgressIndicator()))
            : ListView(
                padding: EdgeInsets.only(bottom: listBottom(context)),
                children: [
                  const GroupHeader('Columns', caps: true),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 0, 32, 8),
                    child: Text('These columns apply to every project. Each has a fixed role in the workflow; you can rename and reorder them.', style: TextStyle(color: Tokens.label2, fontSize: 13)),
                  ),
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 16),
                    child: ClipRRect(
                      borderRadius: BorderRadius.circular(Tokens.cardRadius),
                      child: Material(
                        color: Tokens.cell,
                        child: ReorderableListView(
                          key: const ValueKey('board-columns'),
                          shrinkWrap: true,
                          physics: const NeverScrollableScrollPhysics(),
                          buildDefaultDragHandles: false,
                          onReorderItem: online && !_saving ? _reorder : (_, _) {},
                          children: [
                            for (final (i, c) in columns.indexed)
                              Tooltip(
                                key: ValueKey('column-${c.id}'),
                                message: 'This column has a role in the workflow and cannot be removed.',
                                child: ListTile(
                                  title: Text(c.name),
                                  subtitle: Text([c.kind.role, if (c.kind == ColumnKind.planReview) 'Used only in Plan mode', if (c.kind == ColumnKind.testing) 'Used only with a tester'].join(' · '), style: TextStyle(color: Tokens.label2)),
                                  onTap: online && !_saving ? () => _rename(c) : null,
                                  trailing: ReorderableDragStartListener(index: i, child: const Icon(Icons.drag_handle, semanticLabel: 'Reorder')),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  const GroupHeader('Dispatcher', caps: true),
                  InsetGroup(children: [
                    GroupRow(
                      key: const ValueKey('company-dispatcher'),
                      leading: const IconTile(CupertinoIcons.shuffle),
                      title: 'Default dispatcher',
                      value: dispatcher ?? 'None',
                      onTap: online ? () => _pickDispatcher(company) : null,
                    ),
                  ]),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                    child: Text(
                      archived
                          ? '$dispatcher is archived. Started tickets wait in Todo until a board member chooses an owner.'
                          : 'Chooses an owner for tickets that start without one. Projects can set their own on the project page.',
                      style: TextStyle(color: archived ? Tokens.warning : Tokens.label2, fontSize: 13),
                    ),
                  ),
                  if (testing) ...[
                    const GroupHeader('Tester', caps: true),
                    InsetGroup(children: [
                      GroupRow(
                        key: const ValueKey('company-tester'),
                        leading: const IconTile(CupertinoIcons.checkmark_seal),
                        title: 'Tester',
                        value: tester ?? 'None',
                        onTap: online ? () => _pickTester(company) : null,
                      ),
                    ]),
                    Padding(
                      padding: const EdgeInsets.fromLTRB(32, 8, 32, 0),
                      child: Text(
                        testerArchived
                            ? '$tester is archived. Choose another tester, or None.'
                            : tester == null
                                ? 'None: no Testing step. Reported tickets go straight to Review.'
                                : 'Runs the full test suite once for every reported ticket. Green goes on to Review; red goes back to Doing.',
                        style: TextStyle(color: testerArchived ? Tokens.warning : Tokens.label2, fontSize: 13),
                      ),
                    ),
                  ],
                ],
              ),
      ),
    );
  }
}

/// Rename one column: not empty, not the name of another (§12.1).
class _RenameDialog extends StatefulWidget {
  const _RenameDialog({required this.column, required this.others});

  final BoardColumnView column;
  final List<String> others;

  @override
  State<_RenameDialog> createState() => _RenameDialogState();
}

class _RenameDialogState extends State<_RenameDialog> {
  late final _field = TextEditingController(text: widget.column.name);

  @override
  void dispose() {
    _field.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final text = _field.text.trim();
    final taken = widget.others.any((n) => n.toLowerCase() == text.toLowerCase());
    final error = text.isEmpty ? 'A column needs a name.' : taken ? 'Another column has this name.' : null;
    return AlertDialog(
      title: Text('Rename ${widget.column.name}'),
      content: TextField(
        key: const ValueKey('column-name'),
        controller: _field,
        autofocus: true,
        maxLength: 40,
        onChanged: (_) => setState(() {}),
        decoration: InputDecoration(labelText: 'Name', errorText: error, helperText: 'Role: ${widget.column.kind.role}'),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
        FilledButton(key: const ValueKey('column-name-save'), onPressed: error == null ? () => Navigator.pop(context, text) : null, child: const Text('Save')),
      ],
    );
  }
}
