import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:image_picker/image_picker.dart';
import 'package:intl/intl.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes, maxTaskFiles;
import 'detail_row.dart';
import 'ticket_actions.dart';
import 'ticket_page.dart' show parseLabels;

/// The longest title a ticket takes (MU1-07, as tasks).
const ticketTitleMax = 160;

/// "New ticket" (§10.2, MU1-07): title and project required; priority, owner,
/// labels, a photo or file; mode and deadline under Advanced. Create puts it
/// in Backlog; "Create and start" also starts it (D-09).
class NewTicketPage extends StatefulWidget {
  const NewTicketPage({super.key, this.projectId});

  /// The board's project filter, when set.
  final String? projectId;

  @override
  State<NewTicketPage> createState() => _NewTicketPageState();
}

class _NewTicketPageState extends State<NewTicketPage> {
  final _title = TextEditingController();
  final _description = TextEditingController();
  final _labels = TextEditingController();
  final _titleFocus = FocusNode();
  late String? _project = widget.projectId;
  TicketPriority _priority = TicketPriority.normal;
  String? _owner;
  AgentMode? _mode;
  DateTime? _deadline;
  final _files = <({String name, String path})>[];
  bool _advanced = false;
  bool _tried = false;
  bool _busy = false;
  bool _uploading = false;

  @override
  void initState() {
    super.initState();
    _title.addListener(() => setState(() {}));
    // One open project: it is the project.
    final open = context.read<CompanyBloc>().state.openProjects;
    if (_project == null && open.length == 1) _project = open.single.id;
  }

  @override
  void dispose() {
    _title.dispose();
    _description.dispose();
    _labels.dispose();
    _titleFocus.dispose();
    super.dispose();
  }

  String? get _titleError {
    if (!_tried) return null;
    if (_title.text.trim().isEmpty) return 'Required.';
    return null;
  }

  String? get _projectError => _tried && _project == null ? 'Choose a project.' : null;

  Future<void> _create({required bool start}) async {
    setState(() => _tried = true);
    final title = _title.text.trim();
    if (title.isEmpty) {
      _titleFocus.requestFocus();
      return;
    }
    if (_project == null || _busy) return;
    final repository = context.read<AureliusRepository>();
    final router = GoRouter.of(context);
    final messenger = ScaffoldMessenger.of(context);
    setState(() => _busy = true);
    try {
      final receipt = await repository.write(ticketCreate(
        title: title,
        projectId: _project!,
        description: _description.text.trim().isEmpty ? null : _description.text.trim(),
        priority: _priority == TicketPriority.normal ? null : _priority,
        ownerEmployeeId: _owner,
        labels: parseLabels(_labels.text),
        files: [for (final f in _files) f.path],
        deadlineMs: _deadline?.millisecondsSinceEpoch,
        run: _mode == null ? null : TaskRun(mode: _mode),
        start: start ? true : null,
      ));
      final number = switch (receipt.command) { CommandResultTicketCreated(:final number) => number, _ => null };
      final ticketId = switch (receipt.command) { CommandResultTicketCreated(:final ticketId) => ticketId, _ => null };
      messenger.showSnackBar(SnackBar(
        content: Text(start ? '#$number created and started.' : '#$number created in Backlog.'),
        action: ticketId == null ? null : SnackBarAction(label: 'Open', onPressed: () => router.push('/ticket/$ticketId')),
      ));
      if (mounted && context.canPop()) context.pop();
    } on Object catch (e) {
      if (mounted) toast(context, ticketFailure(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _pickOwner(CompanyState company) async {
    final dispatcher = _project == null ? null : dispatcherName(company, _project!);
    final groups = pickerGroups(company.employees.where((e) => e.status == LifecycleStatus.active).toList());
    final picked = await showModalBottomSheet<(String?,)>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
          child: ListView(shrinkWrap: true, children: [
            ListTile(
              key: const ValueKey('new-owner-dispatcher'),
              title: Text(dispatcher == null ? 'Dispatcher' : 'Dispatcher ($dispatcher)'),
              trailing: _owner == null ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
            for (final g in groups) ...[
              Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow(g.title)),
              for (final e in g.employees)
                ListTile(key: ValueKey('new-owner-${e.id}'), title: Text(e.name), subtitle: Text(e.role), trailing: _owner == e.id ? Icon(Icons.check, color: Tokens.accentText) : null, onTap: () => Navigator.pop(sheet, (e.id,))),
            ],
          ]),
        ),
      ),
    );
    if (picked != null) setState(() => _owner = picked.$1);
  }

  /// A photo or a file, uploaded to the PC at once and sent with the ticket.
  Future<void> _attach() async {
    if (_files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per ticket.');
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
          ListTile(leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
          ListTile(leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
        ]),
      ),
    );
    if (choice == null || !mounted) return;
    final List<(File, String)> picked;
    if (choice == 'file') {
      picked = [for (final f in await FilePicker.pickFiles()) if (f.path != null) (File(f.path!), f.name)];
    } else if (choice == 'gallery') {
      picked = [for (final i in await ImagePicker().pickMultiImage(imageQuality: 85, maxWidth: 2400, limit: maxTaskFiles - _files.length)) (File(i.path), i.name)];
    } else {
      final image = await ImagePicker().pickImage(source: ImageSource.camera, imageQuality: 85, maxWidth: 2400);
      picked = [if (image != null) (File(image.path), image.name)];
    }
    if (picked.isEmpty || !mounted) return;
    final repository = context.read<AureliusRepository>();
    setState(() => _uploading = true);
    try {
      for (final (file, name) in picked.take(maxTaskFiles - _files.length)) {
        if (await file.length() > maxTaskFileBytes) {
          if (mounted) toast(context, '$name is larger than 25 MB and was not added.');
          continue;
        }
        try {
          final uploaded = await repository.upload(file, name);
          if (mounted) setState(() => _files.add((name: uploaded.name, path: uploaded.path)));
        } catch (e) {
          if (mounted) toast(context, '$name: $e');
        }
      }
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  Future<void> _pickDeadline() async {
    final now = DateTime.now();
    final day = await showDatePicker(context: context, initialDate: _deadline ?? now, firstDate: now.subtract(const Duration(days: 1)), lastDate: DateTime(now.year + 10));
    if (day != null) setState(() => _deadline = DateTime(day.year, day.month, day.day, 17));
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final online = context.select<ConnectionBloc, bool>((b) => b.state.isOnline);
    final me = context.select<ConnectionBloc, String?>((b) => b.state.user?.username);
    final dispatcher = _project == null ? null : dispatcherName(company, _project!);
    final owner = employeeName(company, _owner);
    final can = online && !_busy && !_uploading;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): () => _create(start: false),
        const SingleActivator(LogicalKeyboardKey.enter, control: true): () => _create(start: false),
      },
      child: Scaffold(
        appBar: GlassAppBar(
          title: const Text('New ticket'),
          actions: [
            TextButton(key: const ValueKey('new-ticket-create'), onPressed: can ? () => _create(start: false) : null, child: const Text('Create')),
          ],
        ),
        body: ListView(
          padding: EdgeInsets.fromLTRB(0, 8, 0, listBottom(context)),
          children: [
            InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8), children: [
              TextField(
                key: const ValueKey('new-ticket-title'),
                controller: _title,
                focusNode: _titleFocus,
                autofocus: true,
                maxLength: ticketTitleMax,
                textCapitalization: TextCapitalization.sentences,
                buildCounter: (context, {required currentLength, required isFocused, maxLength}) => currentLength >= 140 ? Text('$currentLength/$maxLength') : null,
                decoration: InputDecoration(labelText: 'Title *', border: InputBorder.none, errorText: _titleError),
              ),
              TextField(
                key: const ValueKey('new-ticket-description'),
                controller: _description,
                minLines: 3,
                maxLines: 10,
                textCapitalization: TextCapitalization.sentences,
                decoration: const InputDecoration(labelText: 'Description', border: InputBorder.none, alignLabelWithHint: true),
              ),
            ]),
            const GroupHeader('Project *', caps: true),
            SizedBox(
              height: 44,
              child: ListView(scrollDirection: Axis.horizontal, padding: const EdgeInsets.symmetric(horizontal: 16), children: [
                for (final p in company.openProjects)
                  Padding(
                    padding: const EdgeInsets.only(right: 8),
                    child: ChoiceChip(key: ValueKey('new-ticket-project-${p.id}'), label: Text(p.name), selected: _project == p.id, showCheckmark: false, selectedColor: Tokens.accentTint, onSelected: (_) => setState(() => _project = p.id)),
                  ),
              ]),
            ),
            if (_projectError != null) Padding(padding: const EdgeInsets.fromLTRB(32, 4, 32, 0), child: Text(_projectError!, style: TextStyle(color: Tokens.negative, fontSize: 13))),
            const GroupHeader('Priority', caps: true),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: PillSegments<TicketPriority>(
                segments: [for (final p in ticketPriorities) (p, p.label)],
                selected: _priority,
                onChanged: (p) => setState(() => _priority = p),
              ),
            ),
            const SizedBox(height: 12),
            InsetGroup(children: [
              DetailRow(key: const ValueKey('new-ticket-owner'), label: 'Owner', value: owner ?? (dispatcher == null ? 'Dispatcher' : 'Dispatcher ($dispatcher)'), onTap: () => _pickOwner(company)),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 4),
                child: TextField(key: const ValueKey('new-ticket-labels'), controller: _labels, decoration: const InputDecoration(labelText: 'Labels', hintText: 'backend, security', border: InputBorder.none)),
              ),
              GroupRow(
                key: const ValueKey('new-ticket-attach'),
                leading: _uploading ? const SizedBox(width: 20, height: 20, child: CircularProgressIndicator(strokeWidth: 2)) : const Icon(CupertinoIcons.paperclip),
                title: 'Photo or file',
                value: _files.isEmpty ? null : '${_files.length}',
                onTap: can ? _attach : null,
              ),
              for (final f in _files)
                GroupRow(title: f.name, trailing: IconButton(tooltip: 'Remove', icon: const Icon(Icons.close, size: 18), onPressed: () => setState(() => _files.remove(f)))),
            ]),
            const SizedBox(height: 12),
            InsetGroup(children: [
              DetailRow(key: const ValueKey('new-ticket-advanced'), label: 'Advanced', value: _advanced ? '' : [if (_mode != null) _mode!.label, if (_deadline != null) DateFormat.MMMd().format(_deadline!)].join(' · '), onTap: () => setState(() => _advanced = !_advanced)),
              if (_advanced) ...[
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('Mode', style: TextStyle(color: Tokens.label2, fontSize: 13)),
                    const SizedBox(height: 6),
                    Wrap(spacing: 8, children: [
                      ChoiceChip(key: const ValueKey('new-ticket-mode-default'), label: const Text('Default'), selected: _mode == null, showCheckmark: false, onSelected: (_) => setState(() => _mode = null)),
                      for (final m in agentModes) ChoiceChip(key: ValueKey('new-ticket-mode-${m.wire}'), label: Text(m.label), selected: _mode == m, showCheckmark: false, onSelected: (_) => setState(() => _mode = m)),
                    ]),
                    if (_mode == AgentMode.plan) Padding(padding: const EdgeInsets.only(top: 6), child: Text('Stops in Plan review with a plan before anything changes.', style: TextStyle(color: Tokens.label2, fontSize: 13))),
                  ]),
                ),
                DetailRow(key: const ValueKey('new-ticket-deadline'), label: 'Deadline', value: _deadline == null ? 'None' : DateFormat.yMMMd().format(_deadline!), onTap: _pickDeadline),
                if (_deadline != null) TextButton(onPressed: () => setState(() => _deadline = null), child: const Text('Clear deadline')),
              ],
            ]),
            Padding(
              padding: const EdgeInsets.fromLTRB(32, 16, 32, 8),
              child: Text('Lands in Backlog. Nothing starts until you press Start.${me == null ? '' : ' Created by $me.'}', style: TextStyle(color: Tokens.label2, fontSize: 13)),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: OutlinedButton(key: const ValueKey('new-ticket-create-start'), onPressed: can ? () => _create(start: true) : null, child: const Text('Create and start')),
            ),
          ],
        ),
      ),
    );
  }
}
