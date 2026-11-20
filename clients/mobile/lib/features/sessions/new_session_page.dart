import 'dart:convert';
import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';
import 'package:image_picker/image_picker.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/ios.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/discussion.dart' show TicketDraft;
import '../../data/run_config.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart' show myTicketsFeature;
import '../../data/views.dart';
import '../chat/secret_check.dart' show sendAnywayDialog;
import '../chat/upload_files.dart' show isImageName;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes, maxTaskFiles;
import '../runtime/runtime_bloc.dart';
import 'session_rules.dart';

/// The label of a run choice left to the employee (desktop: "Employee's default").
const employeeDefault = "Employee's default";

/// What the Chat tab starts: a work session, or a ticket (through New ticket).
enum StartKind { session, ticket }

/// A ticket's draft from what was typed in the Chat tab: the first line is its
/// title, the rest its description.
TicketDraft ticketDraftOf(String text, String? projectId) {
  final trimmed = text.trim();
  final at = trimmed.indexOf('\n');
  final title = at < 0 ? trimmed : trimmed.substring(0, at).trim();
  final description = at < 0 ? '' : trimmed.substring(at + 1).trim();
  return TicketDraft(title: title, description: description, projectId: projectId);
}

/// The Chat tab (desktop's New session): "What should happen next?", a row of
/// projects (recently used first), your tickets to continue, and at the bottom
/// the composer with who takes it, model, effort and mode. Start opens the
/// session; Ticket hands the text to New ticket. A server without work
/// sessions only offers the ticket.
class NewSessionPage extends StatefulWidget {
  const NewSessionPage({super.key});

  @override
  State<NewSessionPage> createState() => _NewSessionPageState();
}

class _NewSessionPageState extends State<NewSessionPage> {
  final _text = TextEditingController();
  String? _project;
  String? _who;
  StartKind _kind = StartKind.session;
  List<String> _recent = const [];
  List<TicketView> _continue = const [];
  String _model = 'default';
  String _effort = 'default';
  String _mode = 'default';
  final _images = <({String name, String path})>[];
  final _files = <({String name, String path})>[];
  bool _uploading = false;
  bool _busy = false;
  bool _tried = false;
  bool? _shown;

  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
    _loadRemembered();
  }

  /// The tab reads your tickets to continue each time it shows again.
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final shown = TickerMode.valuesOf(context).enabled;
    if (shown == _shown) return;
    _shown = shown;
    if (shown) _loadContinue();
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  String get _host => context.read<AureliusRepository>().host.hostId;

  /// The project, Who, kind and recent projects last used on this server.
  Future<void> _loadRemembered() async {
    final host = _host;
    final company = context.read<CompanyBloc>().state;
    final dispatchOn = context.read<ConnectionBloc>().state.has(sessionDispatchFeature);
    String? project;
    String? who;
    String? kind;
    var recent = const <String>[];
    try {
      final prefs = await SharedPreferences.getInstance();
      project = prefs.getString(sessionKey('project', host));
      who = prefs.getString(sessionKey('who', host));
      kind = prefs.getString(sessionKey('kind', host));
      recent = _parseRecent(prefs.getString(sessionKey('recent', host)));
    } catch (_) {
      // No preferences (tests, a broken store): the defaults.
    }
    if (!mounted) return;
    setState(() {
      _recent = recent;
      _project ??= defaultSessionProject([for (final p in company.openProjects) p.id], project);
      _who ??= defaultWho(last: who, active: {for (final e in company.activeEmployees) e.id}, dispatchOn: dispatchOn);
      if (kind == StartKind.ticket.name) _kind = StartKind.ticket;
    });
  }

  static List<String> _parseRecent(String? raw) {
    if (raw == null) return const [];
    try {
      final value = jsonDecode(raw);
      return value is List ? [for (final v in value) if (v is String) v] : const [];
    } on FormatException {
      return const [];
    }
  }

  Future<void> _loadContinue() async {
    final repository = context.read<AureliusRepository>();
    if (!repository.linkState.has(myTicketsFeature)) return;
    try {
      final mine = await repository.myTickets();
      if (mounted) setState(() => _continue = continueTickets(mine.tickets));
    } on Object {
      // Nothing to continue shown; the composer works without it.
    }
  }

  Future<void> _remember({required String projectId, String? who}) async {
    final recent = noteRecent(_recent, projectId);
    setState(() => _recent = recent);
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(sessionKey('project', _host), projectId);
      await prefs.setString(sessionKey('recent', _host), jsonEncode(recent));
      await prefs.setString(sessionKey('kind', _host), _kind.name);
      if (who != null) await prefs.setString(sessionKey('who', _host), who);
    } catch (_) {
      // Not saved; the next start picks again.
    }
  }

  void _reset() => setState(() {
        _text.clear();
        _images.clear();
        _files.clear();
        _tried = false;
      });

  bool get _sessionsOn => context.read<ConnectionBloc>().state.has(workSessionsFeature);

  StartKind get _effectiveKind => _sessionsOn ? _kind : StartKind.ticket;

  Future<void> _start() async {
    if (_effectiveKind == StartKind.ticket) return _startTicket();
    setState(() => _tried = true);
    final link = context.read<ConnectionBloc>().state;
    final dispatchOn = link.has(sessionDispatchFeature);
    final problem = startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: link.has(chatFilesFeature));
    final project = _project;
    final who = _who;
    if (problem != null || _busy || _uploading || project == null || who == null) return;
    final repository = context.read<AureliusRepository>();
    final router = GoRouter.of(context);
    final start = SessionStartInput(
      projectId: project,
      text: _text.text,
      model: _model,
      effort: _effort,
      mode: _mode,
      images: [for (final i in _images) i.path],
      files: [for (final f in _files) f.path],
    );
    final request = who == dispatcherWho
        ? workSessionDispatch(sessionDispatchParams(start, confirmSecret: false))
        : workSessionStart(sessionStartParams(start, employeeId: who, confirmSecret: false));
    setState(() => _busy = true);
    try {
      final receipt = await writeConfirmingSecret(repository, request, sendAnywayDialog(context), retry: false);
      if (receipt == null) return;
      await _remember(projectId: project, who: who);
      final conversation = startedSession(receipt);
      _reset();
      if (conversation != null) router.push('/session/$conversation');
    } on ProtocolException catch (e) {
      if (mounted) toast(context, e.shown);
    } on Object catch (e) {
      if (mounted) toast(context, '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// A ticket is made in New ticket, with what was typed here as its draft.
  Future<void> _startTicket() async {
    final project = _project;
    if (project != null) await _remember(projectId: project);
    if (!mounted) return;
    final query = project == null ? '' : '?project=$project';
    await context.push('/board/new$query', extra: ticketDraftOf(_text.text, project));
  }

  /// A photo goes with the first message as an image; any other file only on a server with `chat_files`.
  Future<void> _attach(bool filesOn) async {
    if (_images.length + _files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per session.');
    final choice = await showModalBottomSheet<String>(
      context: context,
      useRootNavigator: true,
      builder: (sheet) => SafeArea(
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          ListTile(leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
          ListTile(leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
          if (filesOn) ListTile(leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
        ]),
      ),
    );
    if (choice == null || !mounted) return;
    final left = maxTaskFiles - _images.length - _files.length;
    final List<(File, String)> picked;
    if (choice == 'file') {
      picked = [for (final f in await FilePicker.pickFiles()) if (f.path case final path?) (File(path), f.name)];
    } else if (choice == 'gallery') {
      picked = [for (final i in await ImagePicker().pickMultiImage(imageQuality: 85, maxWidth: 2400, limit: left)) (File(i.path), i.name)];
    } else {
      final image = await ImagePicker().pickImage(source: ImageSource.camera, imageQuality: 85, maxWidth: 2400);
      picked = [if (image != null) (File(image.path), image.name)];
    }
    if (picked.isEmpty || !mounted) return;
    final repository = context.read<AureliusRepository>();
    setState(() => _uploading = true);
    try {
      for (final (file, name) in picked.take(left)) {
        if (await file.length() > maxTaskFileBytes) {
          if (mounted) toast(context, '$name is larger than 25 MB and was not added.');
          continue;
        }
        try {
          final uploaded = await repository.upload(file, name);
          final entry = (name: uploaded.name, path: uploaded.path);
          if (mounted) setState(() => (choice != 'file' || isImageName(name) ? _images : _files).add(entry));
        } catch (e) {
          if (mounted) toast(context, '$name: $e');
        }
      }
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  Future<void> _pick<T>(String title, List<(T, String)> options, T current, ValueChanged<T> chosen) async {
    final picked = await showModalBottomSheet<(T,)>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow(title)),
            for (final (value, label) in options)
              ListTile(
                key: ValueKey('pick-$title-$value'),
                title: Text(label),
                trailing: value == current ? Icon(Icons.check, color: Tokens.accentText) : null,
                onTap: () => Navigator.pop(sheet, (value,)),
              ),
          ]),
        ),
      ),
    );
    if (picked != null && mounted) setState(() => chosen(picked.$1));
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final sessionsOn = link.has(workSessionsFeature);
    final kind = sessionsOn ? _kind : StartKind.ticket;
    final dispatchOn = link.has(sessionDispatchFeature);
    final filesOn = link.has(chatFilesFeature);
    final models = context.select<RuntimeBloc, List<String>>((b) => b.state.choice(Purpose.employee)?.connection.models ?? const []);
    final projects = company.openProjects;
    final employees = company.activeEmployees;
    final whoName = _who == dispatcherWho ? 'Dispatcher' : company.employee(_who ?? '')?.name;
    final problem = kind == StartKind.ticket
        ? null
        : startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: filesOn);
    final note = kind == StartKind.ticket
        ? 'Opens New ticket with this text, to add an owner, labels and more.'
        : _who == dispatcherWho
            ? dispatchedNote(false)
            : sessionNote(whoName, false);
    final can = !_busy && !_uploading && (kind == StartKind.ticket ? _text.text.trim().isNotEmpty : problem == null);
    final rows = projectRows(projects, _recent, _project, id: (p) => p.id);
    final keyboard = MediaQuery.viewInsetsOf(context).bottom > 0;

    return Scaffold(
      appBar: GlassAppBar(title: const Text('Chat')),
      body: Column(children: [
        Expanded(
          child: ListView(
            key: const ValueKey('chat-body'),
            padding: const EdgeInsets.fromLTRB(0, 8, 0, 16),
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 16, 20, 4),
                child: Text('What should happen next?', key: const ValueKey('chat-heading'), style: TextStyle(fontSize: 26, fontWeight: FontWeight.w700, color: Tokens.label)),
              ),
              const Eyebrow('Project'),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Wrap(spacing: 8, runSpacing: 8, children: [
                  for (final p in rows.rows)
                    ChoiceChip(
                      key: ValueKey('chat-project-${p.id}'),
                      label: Text(p.name),
                      selected: p.id == _project,
                      showCheckmark: false,
                      onSelected: (_) => setState(() => _project = p.id),
                    ),
                  if (rows.more)
                    ActionChip(
                      key: const ValueKey('chat-project-all'),
                      label: const Text('All projects…'),
                      onPressed: () => _pick<String?>('Project', [for (final p in projects) (p.id, p.name)], _project, (v) => _project = v),
                    ),
                  if (projects.isEmpty) Text('No open projects yet.', style: TextStyle(color: Tokens.label2)),
                ]),
              ),
              if (_tried && _project == null && kind == StartKind.session)
                Padding(padding: const EdgeInsets.fromLTRB(20, 6, 20, 0), child: Text('Choose a project.', style: TextStyle(fontSize: 13, color: Tokens.negative))),
              if (_continue.isNotEmpty) ...[
                const Eyebrow('Continue'),
                InsetGroup(children: [
                  for (final t in _continue)
                    GroupRow(
                      key: ValueKey('chat-continue-${t.id}'),
                      title: '#${t.number} ${t.title}',
                      subtitle: t.projectName,
                      value: continueColumnLabel(t.column),
                      chevron: true,
                      onTap: () => context.push('/ticket/${t.id}'),
                    ),
                ]),
              ],
            ],
          ),
        ),
        _composer(context, kind: kind, sessionsOn: sessionsOn, dispatchOn: dispatchOn, filesOn: filesOn, models: models, employees: employees, whoName: whoName, problem: problem, note: note, can: can, keyboard: keyboard),
      ]),
    );
  }

  Widget _composer(
    BuildContext context, {
    required StartKind kind,
    required bool sessionsOn,
    required bool dispatchOn,
    required bool filesOn,
    required List<String> models,
    required List<EmployeeSummary> employees,
    required String? whoName,
    required String? problem,
    required String note,
    required bool can,
    required bool keyboard,
  }) {
    // Above the tab bar; with the keyboard up the bar is under it, so only a margin.
    final bottom = keyboard ? 8.0 : MediaQuery.paddingOf(context).bottom + 8;
    return Container(
      key: const ValueKey('chat-composer'),
      margin: EdgeInsets.fromLTRB(12, 0, 12, bottom),
      padding: const EdgeInsets.fromLTRB(12, 8, 8, 8),
      decoration: BoxDecoration(color: Tokens.cell, borderRadius: BorderRadius.circular(22), border: Border.all(color: Tokens.sep)),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (_images.isNotEmpty || _files.isNotEmpty || _uploading)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Wrap(spacing: 6, runSpacing: 6, children: [
              for (final (list, f) in [for (final i in _images) (_images, i), for (final f in _files) (_files, f)])
                InputChip(
                  key: ValueKey('new-session-file-${f.path}'),
                  avatar: Icon(list == _images ? Icons.image_outlined : Icons.description_outlined, size: 16),
                  label: Text(f.name, overflow: TextOverflow.ellipsis),
                  onDeleted: () => setState(() => list.remove(f)),
                ),
              if (_uploading) const Padding(padding: EdgeInsets.all(8), child: SizedBox.square(dimension: 16, child: CircularProgressIndicator(strokeWidth: 2))),
            ]),
          ),
        Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
          if (kind == StartKind.session)
            IconButton(
              key: const ValueKey('new-session-attach'),
              tooltip: filesOn ? 'Add photos or files' : 'Add photos',
              onPressed: _busy || _uploading ? null : () => _attach(filesOn),
              icon: const Icon(CupertinoIcons.paperclip),
            ),
          Expanded(
            child: TextField(
              key: const ValueKey('new-session-text'),
              controller: _text,
              minLines: 1,
              maxLines: 8,
              textCapitalization: TextCapitalization.sentences,
              decoration: InputDecoration(hintText: kind == StartKind.ticket ? 'Describe the ticket: the first line is its title' : newSessionHint, border: InputBorder.none, isDense: true),
            ),
          ),
          IconButton.filled(
            key: const ValueKey('new-session-start'),
            tooltip: kind == StartKind.ticket ? 'Continue in New ticket' : 'Start',
            onPressed: can ? _start : (_busy || _uploading ? null : () => setState(() => _tried = true)),
            icon: _busy ? const SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2)) : const Icon(CupertinoIcons.arrow_up),
          ),
        ]),
        const SizedBox(height: 4),
        SizedBox(
          height: 36,
          // Every choice built, as the Board's filters: a lazy list would leave some out for screen readers.
          child: SingleChildScrollView(scrollDirection: Axis.horizontal, child: Row(children: [
            if (sessionsOn)
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: CupertinoSlidingSegmentedControl<StartKind>(
                  key: const ValueKey('chat-kind'),
                  groupValue: kind,
                  children: const {StartKind.session: Text('Session'), StartKind.ticket: Text('Ticket')},
                  onValueChanged: (v) => setState(() => _kind = v ?? StartKind.session),
                ),
              ),
            if (kind == StartKind.session) ...[
              _TrayChip(
                key: const ValueKey('new-session-who'),
                label: whoName ?? 'Who?',
                warn: _tried && whoName == null,
                onTap: () => _pick<String?>('Who', [if (dispatchOn) (dispatcherWho, 'Dispatcher'), for (final e in employees) (e.id, e.name)], _who, (v) => _who = v),
              ),
              _TrayChip(
                key: const ValueKey('new-session-model'),
                label: _model == 'default' ? 'Model' : ModelCatalog.friendly(_model),
                onTap: () => _pick('Model', [('default', employeeDefault), for (final m in models) (m, ModelCatalog.friendly(m))], _model, (v) => _model = v),
              ),
              _TrayChip(
                key: const ValueKey('new-session-effort'),
                label: _effort == 'default' ? 'Effort' : ModelCatalog.effortLabel(_effort),
                onTap: () => _pick('Effort', [('default', employeeDefault), for (final (v, label, _) in ModelCatalog.efforts) (v, label)], _effort, (v) => _effort = v),
              ),
              _TrayChip(
                key: const ValueKey('new-session-mode'),
                label: _mode == 'default' ? 'Mode' : AgentMode.fromJson(_mode).label,
                onTap: () => _pick('Mode', [('default', employeeDefault), for (final m in agentModes) (m.wire, m.label)], _mode, (v) => _mode = v),
              ),
            ],
          ])),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 4, 4, 0),
          child: Text(
            _tried && problem != null ? problem : note,
            key: const ValueKey('new-session-note'),
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 12, color: _tried && problem != null ? Tokens.negative : Tokens.label2),
          ),
        ),
      ]),
    );
  }
}

/// One choice in the composer's tray (Who, Model, Effort, Mode).
class _TrayChip extends StatelessWidget {
  const _TrayChip({super.key, required this.label, required this.onTap, this.warn = false});

  final String label;
  final VoidCallback onTap;
  final bool warn;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(right: 6),
        child: ActionChip(
          label: Text(label, style: TextStyle(fontSize: 13, color: warn ? Tokens.negative : null)),
          visualDensity: VisualDensity.compact,
          onPressed: onTap,
        ),
      );
}
