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
import '../my_defaults/my_defaults_bloc.dart' show userRunDefaultsFeature;

/// The label of a run choice left to the employee (desktop: "Employee's default").
const employeeDefault = "Employee's default";

/// The label of a run choice left to the board member's own defaults (feature `user_run_defaults`).
const myDefault = 'My default';

/// The visible gap between the composer and the tab bar's glass.
const _barGap = 10.0;

/// The label of the run pill: the model, effort and mode that differ from the
/// employee's own, or "Default" when none do.
String runSummary({required String model, required String effort, required String mode}) {
  final parts = [
    if (model != 'default') ModelCatalog.friendly(model),
    if (effort != 'default') ModelCatalog.effortLabel(effort),
    if (mode != 'default') AgentMode.fromJson(mode).label,
  ];
  return parts.isEmpty ? 'Default' : parts.join(' · ');
}

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

/// The Chat tab (desktop's New session): the mark and "What should happen
/// next?", the project (a picker, recently used first), Session | Ticket, your
/// tickets to continue, and at the bottom the chips for who takes it, model,
/// effort and mode over a pill-shaped composer. Start opens the
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

  /// The project, Who, kind, recent projects, model, effort and mode last used on this server.
  Future<void> _loadRemembered() async {
    final host = _host;
    final company = context.read<CompanyBloc>().state;
    final dispatchOn = context.read<ConnectionBloc>().state.has(sessionDispatchFeature);
    String? project;
    String? who;
    String? kind;
    String? model;
    String? effort;
    String? mode;
    var recent = const <String>[];
    try {
      final prefs = await SharedPreferences.getInstance();
      project = prefs.getString(sessionKey('project', host));
      who = prefs.getString(sessionKey('who', host));
      kind = prefs.getString(sessionKey('kind', host));
      recent = _parseRecent(prefs.getString(sessionKey('recent', host)));
      model = prefs.getString(sessionKey('model', host));
      effort = prefs.getString(sessionKey('effort', host));
      mode = prefs.getString(sessionKey('mode', host));
    } catch (_) {
      // No preferences (tests, a broken store): the defaults.
    }
    if (!mounted) return;
    setState(() {
      _recent = recent;
      _project ??= defaultSessionProject([for (final p in company.openProjects) p.id], project);
      _who ??= defaultWho(last: who, active: {for (final e in company.activeEmployees) e.id}, dispatchOn: dispatchOn);
      if (kind == StartKind.ticket.name) _kind = StartKind.ticket;
      // The model is checked against the offered list while building, as that list loads later.
      if (_model == 'default') _model = model ?? 'default';
      if (_effort == 'default') _effort = keptChoice(effort, [for (final (v, _, _) in ModelCatalog.efforts) v]);
      if (_mode == 'default') _mode = keptChoice(mode, [for (final m in agentModes) m.wire]);
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

  /// The chosen model while the server still offers it, else the employee's own.
  String _modelOf(List<String> models) => keptChoice(_model, models, loaded: models.isNotEmpty);

  List<String> get _models => context.read<RuntimeBloc>().state.choice(Purpose.employee)?.connection.models ?? const [];

  Future<void> _remember({required String projectId, String? who, SessionStartInput? run}) async {
    final recent = noteRecent(_recent, projectId);
    setState(() => _recent = recent);
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(sessionKey('project', _host), projectId);
      await prefs.setString(sessionKey('recent', _host), jsonEncode(recent));
      await prefs.setString(sessionKey('kind', _host), _kind.name);
      if (who != null) await prefs.setString(sessionKey('who', _host), who);
      if (run != null) {
        await prefs.setString(sessionKey('model', _host), run.model);
        await prefs.setString(sessionKey('effort', _host), run.effort);
        await prefs.setString(sessionKey('mode', _host), run.mode);
      }
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
    final ownBranch = link.has(ownBranchFeature);
    final problem = startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: link.has(chatFilesFeature), ownBranch: ownBranch);
    final project = _project;
    final who = _who;
    if (problem != null || _busy || _uploading || project == null || who == null) return;
    final repository = context.read<AureliusRepository>();
    final router = GoRouter.of(context);
    final start = SessionStartInput(
      projectId: project,
      text: _text.text,
      model: _modelOf(_models),
      effort: _effort,
      mode: _mode,
      images: [for (final i in _images) i.path],
      files: [for (final f in _files) f.path],
    );
    final request = who == dispatcherWho
        ? workSessionDispatch(sessionDispatchParams(start, confirmSecret: false, ownBranch: ownBranch))
        : workSessionStart(sessionStartParams(start, employeeId: who, confirmSecret: false, ownBranch: ownBranch));
    setState(() => _busy = true);
    try {
      final receipt = await writeConfirmingSecret(repository, request, sendAnywayDialog(context), retry: false);
      if (receipt == null) return;
      await _remember(projectId: project, who: who, run: start);
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

  /// The run sheet: model, effort and mode in one slide-up sheet. A choice
  /// applies at once and the sheet stays open, so all three can be set in one go.
  Future<void> _pickRun(List<String> models) async {
    final fallback = context.read<AureliusRepository>().linkState.has(userRunDefaultsFeature) ? myDefault : employeeDefault;
    await showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => StatefulBuilder(
        builder: (sheet, setSheet) {
          void choose(VoidCallback change) {
            setState(change);
            setSheet(() {});
          }

          Widget section(String title, List<(String, String)> options, String current, ValueChanged<String> chosen) => Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 4), child: Eyebrow(title)),
                  for (final (value, label) in options)
                    ListTile(
                      key: ValueKey('pick-$title-$value'),
                      dense: true,
                      title: Text(label),
                      trailing: value == current ? Icon(Icons.check, color: Tokens.accentText) : null,
                      onTap: () => choose(() => chosen(value)),
                    ),
                ],
              );

          return SafeArea(
            child: ConstrainedBox(
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.8),
              child: ListView(shrinkWrap: true, children: [
                section('Model', [('default', fallback), for (final m in models) (m, ModelCatalog.friendly(m))], _modelOf(models), (v) => _model = v),
                section('Effort', [('default', fallback), for (final (v, label, _) in ModelCatalog.efforts) (v, label)], _effort, (v) => _effort = v),
                section('Mode', [('default', fallback), for (final m in agentModes) (m.wire, m.label)], _mode, (v) => _mode = v),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
                  child: FilledButton(key: const ValueKey('run-sheet-done'), onPressed: () => Navigator.pop(sheet), child: const Text('Done')),
                ),
              ]),
            ),
          );
        },
      ),
    );
  }

  /// The project picker: every open project, the recently used first.
  Future<void> _pickProject(BuildContext anchor, List<ProjectSummary> projects) async {
    final ordered = projectRows(projects, _recent, _project, id: (p) => p.id, max: projects.length).rows;
    final picked = await showGlassMenu<String>(
      anchor,
      [for (final p in ordered) GlassMenuItem(p.id, p.name, selected: p.id == _project)],
      matchWidth: true,
    );
    if (picked != null && mounted) setState(() => _project = picked);
  }

  @override
  Widget build(BuildContext context) {
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final sessionsOn = link.has(workSessionsFeature);
    final kind = sessionsOn ? _kind : StartKind.ticket;
    final dispatchOn = link.has(sessionDispatchFeature);
    final filesOn = link.has(chatFilesFeature);
    final ownBranch = link.has(ownBranchFeature);
    final models = context.select<RuntimeBloc, List<String>>((b) => b.state.choice(Purpose.employee)?.connection.models ?? const []);
    final model = _modelOf(models);
    final projects = company.openProjects;
    final employees = company.activeEmployees;
    final whoName = _who == dispatcherWho ? 'Dispatcher' : company.employee(_who ?? '')?.name;
    final problem = kind == StartKind.ticket
        ? null
        : startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: filesOn, ownBranch: ownBranch);
    final note = kind == StartKind.ticket
        ? 'Opens New ticket with this text, to add an owner, labels and more.'
        : _who == dispatcherWho
            ? dispatchedNote(ownBranch)
            : sessionNote(whoName, ownBranch);
    final can = !_busy && !_uploading && (kind == StartKind.ticket ? _text.text.trim().isNotEmpty : problem == null);
    final keyboard = MediaQuery.viewInsetsOf(context).bottom > 0;
    final project = projects.where((p) => p.id == _project).firstOrNull;

    return Scaffold(
      backgroundColor: Tokens.bg,
      body: SafeArea(
        bottom: false,
        child: Column(children: [
          Expanded(
            child: LayoutBuilder(
              builder: (context, box) => SingleChildScrollView(
                key: const ValueKey('chat-body'),
                padding: const EdgeInsets.fromLTRB(20, 16, 20, 16),
                child: ConstrainedBox(
                  constraints: BoxConstraints(minHeight: box.maxHeight - 32),
                  child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      if (!keyboard) const Center(child: AureliusMark(size: 56)),
                      if (!keyboard) const SizedBox(height: 16),
                      Text('What should happen next?',
                          key: const ValueKey('chat-heading'), textAlign: TextAlign.center, style: TextStyle(fontSize: 20, height: 1.3, fontWeight: FontWeight.w600, color: Tokens.label)),
                      const Eyebrow('Project', padding: EdgeInsets.fromLTRB(4, 20, 4, 10)),
                      if (projects.isEmpty)
                        Padding(padding: const EdgeInsets.symmetric(horizontal: 4), child: Text('No open projects yet.', style: TextStyle(color: Tokens.label2)))
                      else
                        Builder(
                          builder: (anchor) => _ProjectField(
                            name: project?.name,
                            warn: _tried && _project == null && kind == StartKind.session,
                            onTap: () => _pickProject(anchor, projects),
                          ),
                        ),
                      if (_tried && _project == null && kind == StartKind.session)
                        Padding(padding: const EdgeInsets.fromLTRB(4, 6, 4, 0), child: Text('Choose a project.', style: TextStyle(fontSize: 13, color: Tokens.negative))),
                      if (sessionsOn) ...[
                        const SizedBox(height: 12),
                        BrandSegments<StartKind>(
                          key: const ValueKey('chat-kind'),
                          segments: const [(StartKind.session, 'Session'), (StartKind.ticket, 'Ticket')],
                          selected: kind,
                          onChanged: (v) => setState(() => _kind = v),
                        ),
                      ],
                      if (_continue.isNotEmpty && !keyboard) ...[
                        const Eyebrow('Continue', padding: EdgeInsets.fromLTRB(4, 24, 4, 8)),
                        InsetGroup(margin: EdgeInsets.zero, children: [
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
              ),
            ),
          ),
          _composer(context, kind: kind, dispatchOn: dispatchOn, filesOn: filesOn, models: models, model: model, employees: employees, whoName: whoName, problem: problem, note: note, can: can, keyboard: keyboard),
        ]),
      ),
    );
  }

  Widget _composer(
    BuildContext context, {
    required StartKind kind,
    required bool dispatchOn,
    required bool filesOn,
    required List<String> models,
    required String model,
    required List<EmployeeSummary> employees,
    required String? whoName,
    required String? problem,
    required String note,
    required bool can,
    required bool keyboard,
  }) {
    // Just above the tab bar's glass; with the keyboard up the bar is under it, so only a margin.
    final bottom = keyboard ? 8.0 : MediaQuery.paddingOf(context).bottom - AppTabBar.clearTop + _barGap;
    final showProblem = _tried && problem != null;
    // Who takes it and how only matters once there is something to start.
    final showNote = showProblem || _text.text.trim().isNotEmpty;
    return Padding(
      key: const ValueKey('chat-composer'),
      padding: EdgeInsets.fromLTRB(20, 0, 20, bottom),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (kind == StartKind.session)
          // Who on the left; model, effort and mode in one pill on the right, whose sheet sets all three.
          Padding(
            padding: const EdgeInsets.fromLTRB(2, 0, 2, 10),
            child: Row(children: [
              Expanded(
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: _TrayChip(
                    key: const ValueKey('new-session-who'),
                    label: whoName ?? 'Who?',
                    warn: _tried && whoName == null,
                    onTap: () => _pick<String?>('Who', [if (dispatchOn) (dispatcherWho, 'Dispatcher'), for (final e in employees) (e.id, e.name)], _who, (v) => _who = v),
                  ),
                ),
              ),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 220),
                child: _TrayChip(
                  key: const ValueKey('new-session-run'),
                  icon: CupertinoIcons.slider_horizontal_3,
                  label: runSummary(model: model, effort: _effort, mode: _mode),
                  onTap: () => _pickRun(models),
                ),
              ),
            ]),
          ),
        if (_images.isNotEmpty || _files.isNotEmpty || _uploading)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
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
        Container(
          padding: const EdgeInsets.all(6),
          decoration: BoxDecoration(
            color: Tokens.cell,
            borderRadius: BorderRadius.circular(26),
            border: Border.all(color: Tokens.sep),
            boxShadow: Tokens.dark ? null : const [BoxShadow(color: Color(0x0A000000), blurRadius: 2, offset: Offset(0, 1)), BoxShadow(color: Color(0x0D000000), blurRadius: 16, offset: Offset(0, 6))],
          ),
          child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
            if (kind == StartKind.session)
              _RoundButton(
                key: const ValueKey('new-session-attach'),
                tooltip: filesOn ? 'Add photos or files' : 'Add photos',
                onPressed: _busy || _uploading ? null : () => _attach(filesOn),
                child: Icon(CupertinoIcons.paperclip, size: 20, color: Tokens.ink2),
              ),
            Expanded(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 9),
                child: TextField(
                  key: const ValueKey('new-session-text'),
                  controller: _text,
                  minLines: 1,
                  maxLines: 6,
                  textCapitalization: TextCapitalization.sentences,
                  style: TextStyle(fontSize: 16, color: Tokens.label),
                  decoration: InputDecoration(
                    hintText: kind == StartKind.ticket ? 'Describe the ticket: the first line is its title' : newSessionHint,
                    hintStyle: TextStyle(fontSize: 16, color: Tokens.label2),
                    border: InputBorder.none,
                    enabledBorder: InputBorder.none,
                    focusedBorder: InputBorder.none,
                    filled: false,
                    isDense: true,
                    contentPadding: EdgeInsets.zero,
                  ),
                ),
              ),
            ),
            _RoundButton(
              key: const ValueKey('new-session-start'),
              tooltip: kind == StartKind.ticket ? 'Continue in New ticket' : 'Start',
              brand: true,
              onPressed: can ? _start : (_busy || _uploading ? null : () => setState(() => _tried = true)),
              child: _busy
                  ? SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2, color: Tokens.accentInk))
                  : Icon(CupertinoIcons.arrow_up, size: 20, color: Tokens.accentInk),
            ),
          ]),
        ),
        if (showNote)
          Padding(
            padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
            child: Text(
              showProblem ? problem : note,
              key: const ValueKey('new-session-note'),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(fontSize: 12, color: showProblem ? Tokens.negative : Tokens.label2),
            ),
          ),
      ]),
    );
  }
}

/// The Aurelius mark: a white chevron on a green rounded square.
class AureliusMark extends StatelessWidget {
  const AureliusMark({super.key, this.size = 56});

  final double size;

  @override
  Widget build(BuildContext context) => Semantics(
        label: 'Aurelius',
        image: true,
        child: CustomPaint(size: Size.square(size), painter: _MarkPainter(Tokens.accent)),
      );
}

class _MarkPainter extends CustomPainter {
  _MarkPainter(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    // Drawn on a 24-unit grid, as the design's SVG.
    final u = size.width / 24;
    canvas.drawRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(6 * u)), Paint()..color = color);
    final chevron = Path()
      ..moveTo(7.5 * u, 15.5 * u)
      ..lineTo(12 * u, 8.5 * u)
      ..lineTo(16.5 * u, 15.5 * u);
    canvas.drawPath(
      chevron,
      Paint()
        ..color = Colors.white
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.6 * u
        ..strokeCap = StrokeCap.round
        ..strokeJoin = StrokeJoin.round,
    );
  }

  @override
  bool shouldRepaint(_MarkPainter old) => old.color != color;
}

/// The chosen project as a glass field with a folder tile; a tap opens the list.
class _ProjectField extends StatelessWidget {
  const _ProjectField({required this.name, required this.onTap, this.warn = false});

  final String? name;
  final VoidCallback onTap;
  final bool warn;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'Project: ${name ?? 'none chosen'}',
        excludeSemantics: true,
        child: GestureDetector(
          key: const ValueKey('chat-project'),
          behavior: HitTestBehavior.opaque,
          onTap: onTap,
          child: Glass(
            radius: 18,
            tint: Tokens.dark ? Tokens.glass : Colors.white.withValues(alpha: 0.6),
            padding: const EdgeInsets.fromLTRB(8, 7, 14, 7),
            child: Row(children: [
              Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(color: Tokens.accentTint, borderRadius: BorderRadius.circular(11)),
                child: Icon(CupertinoIcons.folder, size: 18, color: Tokens.accent),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  name ?? 'Choose a project',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600, color: warn ? Tokens.negative : (name == null ? Tokens.label2 : Tokens.label)),
                ),
              ),
              Icon(CupertinoIcons.chevron_down, size: 16, color: Tokens.label2),
            ]),
          ),
        ),
      );
}

/// A 40 pt round button in the composer: glass, or the accent for Start.
class _RoundButton extends StatelessWidget {
  const _RoundButton({super.key, required this.tooltip, required this.onPressed, required this.child, this.brand = false});

  final String tooltip;
  final VoidCallback? onPressed;
  final Widget child;
  final bool brand;

  @override
  Widget build(BuildContext context) {
    final off = onPressed == null;
    return Tooltip(
      message: tooltip,
      child: Semantics(
        button: true,
        enabled: !off,
        label: tooltip,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: onPressed,
          child: Opacity(
            opacity: off ? 0.5 : 1,
            child: Container(
              width: 40,
              height: 40,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: brand ? Tokens.accent.withValues(alpha: 0.92) : (Tokens.dark ? Tokens.cell2 : Colors.white.withValues(alpha: 0.6)),
                border: Border.all(color: brand ? Colors.white.withValues(alpha: 0.3) : Tokens.sep),
                boxShadow: brand ? [BoxShadow(color: Tokens.accent.withValues(alpha: 0.3), blurRadius: 16, offset: const Offset(0, 6))] : null,
              ),
              child: child,
            ),
          ),
        ),
      ),
    );
  }
}

/// One choice in the composer's tray (Who, Model, Effort, Mode).
class _TrayChip extends StatelessWidget {
  const _TrayChip({super.key, required this.label, required this.onTap, this.warn = false, this.icon});

  final String label;
  final VoidCallback onTap;
  final bool warn;
  final IconData? icon;

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.only(right: 6),
        child: Semantics(
          button: true,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: onTap,
            child: Glass(
              radius: 15,
              flat: true,
              tint: Tokens.dark ? Tokens.glass : Colors.white.withValues(alpha: 0.6),
              padding: const EdgeInsets.fromLTRB(10, 6, 8, 6),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                if (icon != null) Padding(padding: const EdgeInsets.only(right: 5), child: Icon(icon, size: 13, color: Tokens.ink2)),
                Flexible(child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: TextStyle(fontSize: 13, color: warn ? Tokens.negative : Tokens.ink2))),
                const SizedBox(width: 4),
                Icon(CupertinoIcons.chevron_down, size: 11, color: warn ? Tokens.negative : Tokens.ink2),
              ]),
            ),
          ),
        ),
      );
}
