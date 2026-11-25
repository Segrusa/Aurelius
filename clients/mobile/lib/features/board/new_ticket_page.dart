import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:image_picker/image_picker.dart';
import 'package:intl/intl.dart';

import '../my_defaults/run_default_chips.dart';
import '../../core/clipboard/image_clipboard.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/discussion.dart';
import '../../data/quick_task.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../chat/secret_check.dart' show sendAnywayDialog;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes, maxTaskFiles;
import 'detail_row.dart';
import 'ticket_actions.dart';
import 'ticket_page.dart' show parseLabels;

/// The longest title a ticket takes (MU1-07, as tasks).
const ticketTitleMax = 160;

/// "New ticket" (§10.2, MU1-07): title and project required; priority, owner,
/// responsible (You unless chosen otherwise, B9),
/// labels, a photo or file (also pasted into the title or description); mode
/// and deadline under Advanced. Create puts it
/// in Backlog; "Create and start" also starts it (D-09). On a server with
/// quick tasks (ticket #53) it can be a Quick task instead: description and
/// employee required, the title optional, and it starts in Doing at once.
class NewTicketPage extends StatefulWidget {
  const NewTicketPage({super.key, this.projectId, this.draft});

  /// The board's project filter, when set.
  final String? projectId;

  /// Made from a chat message (MU2-10 AC1): title, description and project filled in.
  final TicketDraft? draft;

  @override
  State<NewTicketPage> createState() => _NewTicketPageState();
}

class _NewTicketPageState extends State<NewTicketPage> with WidgetsBindingObserver {
  late final _title = TextEditingController(text: widget.draft?.title);
  late final _description = TextEditingController(text: widget.draft?.description);
  late final _labels = TextEditingController(text: widget.draft?.labels);
  final _titleFocus = FocusNode();
  final _descriptionFocus = FocusNode();
  final _clipboard = ImageClipboard();

  /// Whether the clipboard holds an image, for Paste Image in the edit menu.
  /// Checked when a field gets focus or is tapped and when the app returns,
  /// since the edit menu is built synchronously.
  bool _clipboardImage = false;
  late String? _project = widget.draft?.projectId ?? widget.projectId;
  TicketPriority _priority = TicketPriority.normal;
  late String? _owner = widget.draft?.ownerId;

  /// Chosen in the picker; null: the default, You (B9). Its id is null for No one.
  ({String? id, String label})? _responsible;
  AgentMode? _mode;
  DateTime? _deadline;
  final _files = <({String name, String path})>[];
  bool _advanced = false;

  /// A quick task rather than a ticket (ticket #53); only shown with [quickTaskFeature].
  late bool _quick = widget.draft?.quick ?? false;
  bool _tried = false;
  bool _busy = false;
  bool _uploading = false;

  @override
  void initState() {
    super.initState();
    _title.addListener(() => setState(() {}));
    // A quick task's description is required: its error goes once there is text.
    _description.addListener(() {
      if (_tried && _quick) setState(() {});
    });
    WidgetsBinding.instance.addObserver(this);
    for (final focus in [_titleFocus, _descriptionFocus]) {
      focus.addListener(() {
        if (focus.hasFocus) _checkClipboard();
      });
    }
    // One open project: it is the project.
    final open = context.read<CompanyBloc>().state.openProjects;
    if (_project == null && open.length == 1) _project = open.single.id;
    if (_quick) _fillLastQuickEmployee();
  }

  /// The employee last given a quick task, when none is chosen yet (ST-01 AC2).
  Future<void> _fillLastQuickEmployee() async {
    try {
      await LastQuickEmployee.load(context.read<AureliusRepository>().host.hostId);
    } catch (_) {
      // No host (tests): nothing saved.
    }
    if (!mounted || _owner != null) return;
    final last = LastQuickEmployee.employee;
    if (last != null && context.read<CompanyBloc>().state.activeEmployees.any((e) => e.id == last)) setState(() => _owner = last);
  }

  void _setQuick(bool quick) {
    setState(() {
      _quick = quick;
      _tried = false;
    });
    if (quick) _fillLastQuickEmployee();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) _checkClipboard();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _descriptionFocus.dispose();
    _title.dispose();
    _description.dispose();
    _labels.dispose();
    _titleFocus.dispose();
    super.dispose();
  }

  String? get _titleError {
    if (!_tried || _quick) return null;
    if (_title.text.trim().isEmpty) return 'Required.';
    return null;
  }

  String? get _descriptionError => _tried && _quick && _description.text.trim().isEmpty ? 'Required.' : null;

  String? get _ownerError => _tried && _quick && _owner == null ? 'Choose an employee.' : null;

  String? get _projectError => _tried && _project == null ? 'Choose a project.' : null;

  Future<void> _create({required bool start}) async {
    setState(() => _tried = true);
    if (_quick && quickTasksOn(context.read<AureliusRepository>().linkState)) return _createQuick();
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
      // Only a server of Phase 2 knows `from_message`; an older one refuses unknown fields.
      final from = repository.linkState.has(ticketDiscussionFeature) ? widget.draft?.fromMessage : null;
      // B10 (AUR-MU-04-F5): Send anyway creates it with `confirm_secret`; Cancel keeps the form.
      final receipt = await writeConfirmingSecret(repository, ticketCreate(
        fromMessage: from,
        title: title,
        projectId: _project!,
        description: _description.text.trim().isEmpty ? null : _description.text.trim(),
        priority: _priority == TicketPriority.normal ? null : _priority,
        ownerEmployeeId: _owner,
        // Only a server with the feature knows it; an older one refuses unknown fields. Left out means no one.
        responsibleUserId: repository.linkState.has(ticketResponsibleFeature) ? (_responsible == null ? repository.linkState.user?.id : _responsible!.id) : null,
        labels: parseLabels(_labels.text),
        files: [for (final f in _files) f.path],
        deadlineMs: _deadline?.millisecondsSinceEpoch,
        run: _mode == null ? null : TaskRun(mode: _mode),
        start: start ? true : null,
      ), sendAnywayDialog(context));
      if (receipt == null) return;
      final number = switch (receipt.command) { CommandResultTicketCreated(:final number) => number, _ => null };
      final ticketId = switch (receipt.command) { CommandResultTicketCreated(:final ticketId) => ticketId, _ => null };
      messenger.showSnackBar(SnackBar(
        // P-36 (QA3 M-m2): a new ticket runs on the own login; without one it waits.
        content: Text(start ? '#$number created and started.${waitsForMyLogin(repository.linkState, repository.linkState.user?.id) ? ' $waitsForLoginNote' : ''}' : '#$number created in Backlog.'),
      ));
      _openCreated(router, ticketId);
    } on Object catch (e) {
      if (mounted) toast(context, ticketFailure(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// The new ticket's chat takes this form's place, so Back returns to where
  /// New ticket was opened (ticket #58).
  void _openCreated(GoRouter router, String? ticketId) {
    if (ticketId != null) {
      router.pushReplacement(ticketChatPlace(ticketId));
    } else if (mounted && context.canPop()) {
      context.pop();
    }
  }

  /// Create a quick task (ticket #53): the server starts it in Doing at once.
  /// Its errors ("Dit project heeft geen git-map") show as they come.
  Future<void> _createQuick() async {
    final description = _description.text.trim();
    if (description.isEmpty) {
      _descriptionFocus.requestFocus();
      return;
    }
    final employee = _owner;
    if (_project == null || employee == null || _busy) return;
    final repository = context.read<AureliusRepository>();
    final router = GoRouter.of(context);
    final messenger = ScaffoldMessenger.of(context);
    final name = employeeName(context.read<CompanyBloc>().state, employee) ?? 'its employee';
    setState(() => _busy = true);
    try {
      final link = repository.linkState;
      final receipt = await writeConfirmingSecret(repository, quickTaskRequest(
        projectId: _project!,
        title: _title.text,
        description: description,
        employeeId: employee,
        responsibleUserId: link.has(ticketResponsibleFeature) ? (_responsible == null ? link.user?.id : _responsible!.id) : null,
        priority: _priority == TicketPriority.normal ? null : _priority,
        labels: parseLabels(_labels.text),
        files: [for (final f in _files) f.path],
        deadlineMs: _deadline?.millisecondsSinceEpoch,
        fromMessage: link.has(ticketDiscussionFeature) ? widget.draft?.fromMessage : null,
      ), sendAnywayDialog(context));
      if (receipt == null) return;
      LastQuickEmployee.remember(repository.host.hostId, employee);
      final number = switch (receipt.command) { CommandResultTicketCreated(:final number) => number, _ => null };
      final ticketId = switch (receipt.command) { CommandResultTicketCreated(:final ticketId) => ticketId, _ => null };
      messenger.showSnackBar(SnackBar(
        content: Text('Quick task #$number started with $name.${waitsForMyLogin(link, link.user?.id) ? ' $waitsForLoginNote' : ''}'),
      ));
      _openCreated(router, ticketId);
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
            // A quick task is never dispatched: its employee is chosen here.
            if (!_quick)
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

  Future<void> _pickResponsible() async {
    final me = context.read<ConnectionBloc>().state.user?.id;
    final List<UserView> members;
    try {
      members = await context.read<AureliusRepository>().boardMembers();
    } on Object catch (e) {
      if (mounted) toast(context, ticketFailure(e));
      return;
    }
    if (!mounted) return;
    final picked = await showResponsiblePicker(context, title: 'Responsible', current: _responsible == null ? me : _responsible!.id, members: members, me: me);
    if (picked == null) return;
    final id = picked.$1;
    final label = id == null ? 'No one' : id == me ? 'You' : members.where((u) => u.id == id).firstOrNull?.username ?? 'Board member';
    setState(() => _responsible = (id: id, label: label));
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
    await _upload(picked);
  }

  /// Uploads files to the PC at once; they are sent with the ticket.
  Future<void> _upload(List<(File, String)> picked) async {
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

  Future<void> _checkClipboard() async {
    final has = await _clipboard.hasImage();
    if (mounted && has != _clipboardImage) setState(() => _clipboardImage = has);
  }

  /// Paste the clipboard's image (⌘⇧V on a hardware keyboard) as an attachment.
  Future<void> _pasteImage() async {
    if (_busy || _uploading) return;
    final image = await _clipboard.read();
    if (!mounted) return;
    if (image == null) return toast(context, 'There is no image on the clipboard.');
    await _addImage(image.$1, image.$2);
  }

  /// An image inserted by the keyboard (Gboard) into the title or description.
  Future<void> _insert(KeyboardInsertedContent content) async {
    final data = content.data;
    if (data == null || data.isEmpty) return toast(context, 'This image could not be read; add it with Photo or file.');
    await _addImage(data, pastedImageExtension(content.mimeType));
  }

  /// Keep pasted bytes as a file and attach it like a picked photo.
  Future<void> _addImage(Uint8List data, String extension) async {
    if (_files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per ticket.');
    if (data.length > maxTaskFileBytes) return toast(context, 'The image is larger than 25 MB and was not added.');
    final name = pastedImageName(_files.map((f) => f.name), extension);
    await withPastedFile(data, name, (file) async {
      if (mounted) await _upload([(file, name)]);
    });
  }

  /// The edit menu of the title and description, with Paste Image when the clipboard holds one.
  Widget _menu(BuildContext context, EditableTextState field) {
    final items = _clipboardImage ? withPasteImage(field.contextMenuButtonItems, _pasteImage) : field.contextMenuButtonItems;
    return AdaptiveTextSelectionToolbar.buttonItems(anchors: field.contextMenuAnchors, buttonItems: items);
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
    final responsibleOn = context.select<ConnectionBloc, bool>((b) => b.state.has(ticketResponsibleFeature));
    final dispatcher = _project == null ? null : dispatcherName(company, _project!);
    final owner = employeeName(company, _owner);
    final can = online && !_busy && !_uploading;
    final quickOn = context.select<ConnectionBloc, bool>((b) => quickTasksOn(b.state));
    final quick = quickOn && _quick;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): () => _create(start: false),
        const SingleActivator(LogicalKeyboardKey.enter, control: true): () => _create(start: false),
        const SingleActivator(LogicalKeyboardKey.keyV, meta: true, shift: true): _pasteImage,
        const SingleActivator(LogicalKeyboardKey.keyV, control: true, shift: true): _pasteImage,
      },
      child: Scaffold(
        appBar: GlassAppBar(
          title: Text(quick ? 'New quick task' : 'New ticket'),
          actions: [
            TextButton(key: const ValueKey('new-ticket-create'), onPressed: can ? () => _create(start: false) : null, child: const Text('Create')),
          ],
        ),
        body: ListView(
          padding: EdgeInsets.fromLTRB(0, 8, 0, listBottom(context)),
          children: [
            if (quickOn)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: PillSegments<bool>(
                  key: const ValueKey('new-ticket-kind'),
                  segments: const [(false, 'Ticket'), (true, quickTaskName)],
                  selected: _quick,
                  onChanged: _setQuick,
                ),
              ),
            InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8), children: [
              TextField(
                key: const ValueKey('new-ticket-title'),
                controller: _title,
                focusNode: _titleFocus,
                autofocus: true,
                maxLength: ticketTitleMax,
                textCapitalization: TextCapitalization.sentences,
                onTap: _checkClipboard,
                contextMenuBuilder: _menu,
                contentInsertionConfiguration: ContentInsertionConfiguration(allowedMimeTypes: pastedImageMimeTypes, onContentInserted: _insert),
                buildCounter: (context, {required currentLength, required isFocused, maxLength}) => currentLength >= 140 ? Text('$currentLength/$maxLength') : null,
                decoration: InputDecoration(labelText: quick ? 'Title' : 'Title *', hintText: quick ? 'From the first line of the description' : null, border: InputBorder.none, errorText: _titleError),
              ),
              TextField(
                key: const ValueKey('new-ticket-description'),
                controller: _description,
                focusNode: _descriptionFocus,
                minLines: 3,
                maxLines: 10,
                textCapitalization: TextCapitalization.sentences,
                onTap: _checkClipboard,
                contextMenuBuilder: _menu,
                contentInsertionConfiguration: ContentInsertionConfiguration(allowedMimeTypes: pastedImageMimeTypes, onContentInserted: _insert),
                decoration: InputDecoration(labelText: quick ? 'Description *' : 'Description', border: InputBorder.none, alignLabelWithHint: true, errorText: _descriptionError),
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
              quick
                  ? DetailRow(key: const ValueKey('new-ticket-owner'), label: 'Employee *', value: owner ?? 'Choose', color: _ownerError == null ? null : Tokens.negative, onTap: () => _pickOwner(company))
                  : DetailRow(key: const ValueKey('new-ticket-owner'), label: 'Owner', value: owner ?? (dispatcher == null ? 'Dispatcher' : 'Dispatcher ($dispatcher)'), onTap: () => _pickOwner(company)),
              if (_ownerError != null) Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 8), child: Text(_ownerError!, key: const ValueKey('new-ticket-owner-error'), style: TextStyle(color: Tokens.negative, fontSize: 13))),
              if (responsibleOn) DetailRow(key: const ValueKey('new-ticket-responsible'), label: 'Responsible', value: _responsible?.label ?? 'You', onTap: _pickResponsible),
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
              DetailRow(key: const ValueKey('new-ticket-advanced'), label: 'Advanced', value: _advanced ? '' : [if (_mode != null && !quick) _mode!.label, if (_deadline != null) DateFormat.MMMd().format(_deadline!)].join(' · '), onTap: () => setState(() => _advanced = !_advanced)),
              if (_advanced) ...[
                // A quick task always runs in Auto (ST-01 AC4): no mode to choose.
                if (!quick)
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text('Mode', style: TextStyle(color: Tokens.label2, fontSize: 13)),
                    const SizedBox(height: 6),
                    Wrap(spacing: 8, children: [
                      ChoiceChip(key: const ValueKey('new-ticket-mode-default'), label: Text(watchRunDefaultLabels(context)?.mode ?? 'Default'), selected: _mode == null, showCheckmark: false, onSelected: (_) => setState(() => _mode = null)),
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
              child: Text(
                quick
                    ? 'Starts in Doing at once with the chosen employee, on its own branch. $approveAndMerge brings it into main.${me == null ? '' : ' Created by $me.'}'
                    : 'Lands in Backlog. Nothing starts until you press Start.${me == null ? '' : ' Created by $me.'}',
                key: const ValueKey('new-ticket-note'),
                style: TextStyle(color: Tokens.label2, fontSize: 13),
              ),
            ),
            if (!quick)
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
