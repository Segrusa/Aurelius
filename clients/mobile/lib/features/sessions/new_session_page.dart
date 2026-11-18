import 'dart:io';

import 'package:file_picker/file_picker.dart';
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
import '../../data/run_config.dart';
import '../../data/secret_write.dart';
import '../../data/views.dart';
import '../board/detail_row.dart';
import '../board/new_ticket_page.dart';
import '../chat/secret_check.dart' show sendAnywayDialog;
import '../chat/upload_files.dart' show isImageName;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../project/new_task_page.dart' show maxTaskFileBytes, maxTaskFiles;
import '../runtime/runtime_bloc.dart';
import 'session_rules.dart';

/// The label of a run choice left to the employee (desktop: "Employee's default").
const employeeDefault = "Employee's default";

/// New session (ticket #71, desktop's New session): a project, who takes it
/// (an employee, or the Dispatcher on a server with `session_dispatch`), the
/// model, effort and mode of its runs, the first message and photos or files.
/// Start opens the session. A server without work sessions gets New ticket.
class NewSessionPage extends StatelessWidget {
  const NewSessionPage({super.key});

  @override
  Widget build(BuildContext context) {
    final on = context.select<ConnectionBloc, bool>((b) => b.state.has(workSessionsFeature));
    return on ? const _NewSessionForm() : const NewTicketPage();
  }
}

class _NewSessionForm extends StatefulWidget {
  const _NewSessionForm();

  @override
  State<_NewSessionForm> createState() => _NewSessionFormState();
}

class _NewSessionFormState extends State<_NewSessionForm> {
  final _text = TextEditingController();
  String? _project;
  String? _who;
  String _model = 'default';
  String _effort = 'default';
  String _mode = 'default';
  final _images = <({String name, String path})>[];
  final _files = <({String name, String path})>[];
  bool _uploading = false;
  bool _busy = false;
  bool _tried = false;

  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
    _loadRemembered();
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  String get _host => context.read<AureliusRepository>().host.hostId;

  /// The project and Who last started with on this server.
  Future<void> _loadRemembered() async {
    final host = _host;
    final company = context.read<CompanyBloc>().state;
    final dispatchOn = context.read<ConnectionBloc>().state.has(sessionDispatchFeature);
    String? project;
    String? who;
    try {
      final prefs = await SharedPreferences.getInstance();
      project = prefs.getString(sessionKey('project', host));
      who = prefs.getString(sessionKey('who', host));
    } catch (_) {
      // No preferences (tests, a broken store): the defaults.
    }
    if (!mounted) return;
    setState(() {
      _project ??= defaultSessionProject([for (final p in company.openProjects) p.id], project);
      _who ??= defaultWho(last: who, active: {for (final e in company.activeEmployees) e.id}, dispatchOn: dispatchOn);
    });
  }

  Future<void> _remember(String projectId, String who) async {
    try {
      final prefs = await SharedPreferences.getInstance();
      await prefs.setString(sessionKey('project', _host), projectId);
      await prefs.setString(sessionKey('who', _host), who);
    } catch (_) {
      // Not saved; the next New session picks again.
    }
  }

  Future<void> _start() async {
    setState(() => _tried = true);
    final link = context.read<ConnectionBloc>().state;
    final dispatchOn = link.has(sessionDispatchFeature);
    final problem = startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: link.has(chatFilesFeature));
    if (problem != null || _busy || _uploading) return;
    final repository = context.read<AureliusRepository>();
    final router = GoRouter.of(context);
    final start = SessionStartInput(
      projectId: _project!,
      text: _text.text,
      model: _model,
      effort: _effort,
      mode: _mode,
      images: [for (final i in _images) i.path],
      files: [for (final f in _files) f.path],
    );
    final who = _who!;
    final request = who == dispatcherWho
        ? workSessionDispatch(sessionDispatchParams(start, confirmSecret: false))
        : workSessionStart(sessionStartParams(start, employeeId: who, confirmSecret: false));
    setState(() => _busy = true);
    try {
      final receipt = await writeConfirmingSecret(repository, request, sendAnywayDialog(context), retry: false);
      if (receipt == null) return;
      await _remember(start.projectId, who);
      final conversation = startedSession(receipt);
      if (conversation == null) {
        if (mounted) context.pop();
      } else {
        router.pushReplacement('/session/$conversation');
      }
    } on ProtocolException catch (e) {
      if (mounted) toast(context, e.shown);
    } on Object catch (e) {
      if (mounted) toast(context, '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// A photo goes with the first message as an image; any other file only on a server with `chat_files`.
  Future<void> _attach(bool filesOn) async {
    if (_images.length + _files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per session.');
    final choice = await showModalBottomSheet<String>(
      context: context,
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
      picked = [for (final f in await FilePicker.pickFiles()) if (f.path != null) (File(f.path!), f.name)];
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
    final dispatchOn = link.has(sessionDispatchFeature);
    final filesOn = link.has(chatFilesFeature);
    final models = context.select<RuntimeBloc, List<String>>((b) => b.state.choice(Purpose.employee)?.connection.models ?? const []);
    final projects = company.openProjects;
    final employees = company.activeEmployees;
    final project = company.project(_project ?? '');
    final whoName = _who == dispatcherWho ? 'Dispatcher' : company.employee(_who ?? '')?.name;
    final problem = startProblem(offline: !link.isOnline, projectId: _project, text: _text.text, who: _who, dispatchOn: dispatchOn, files: _files.length, filesOn: filesOn);
    final note = _who == dispatcherWho ? dispatchedNote(false) : sessionNote(whoName, false);
    final can = problem == null && !_busy && !_uploading;
    return Scaffold(
      appBar: GlassAppBar(
        title: const Text('New session'),
        actions: [
          TextButton(key: const ValueKey('new-session-start'), onPressed: _busy || _uploading ? null : _start, child: const Text('Start')),
        ],
      ),
      body: ListView(
        padding: EdgeInsets.fromLTRB(0, 8, 0, listBottom(context)),
        children: [
          InsetGroup(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8), children: [
            TextField(
              key: const ValueKey('new-session-text'),
              controller: _text,
              autofocus: true,
              minLines: 3,
              maxLines: 10,
              textCapitalization: TextCapitalization.sentences,
              decoration: const InputDecoration(hintText: newSessionHint, border: InputBorder.none),
            ),
          ]),
          if (_images.isNotEmpty || _files.isNotEmpty || _uploading)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
              child: Wrap(spacing: 8, runSpacing: 8, children: [
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
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 0),
            child: Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                key: const ValueKey('new-session-attach'),
                onPressed: _busy || _uploading ? null : () => _attach(filesOn),
                icon: const Icon(Icons.attach_file, size: 18),
                label: Text(filesOn ? 'Add photos or files' : 'Add photos'),
              ),
            ),
          ),
          const SizedBox(height: 8),
          InsetGroup(children: [
            DetailRow(
              key: const ValueKey('new-session-project'),
              label: 'Project',
              value: project?.name ?? 'Choose',
              color: _tried && project == null ? Tokens.negative : null,
              onTap: () => _pick<String?>('Project', [for (final p in projects) (p.id, p.name)], _project, (v) => _project = v),
            ),
            DetailRow(
              key: const ValueKey('new-session-who'),
              label: 'Who',
              value: whoName ?? 'Choose',
              color: _tried && whoName == null ? Tokens.negative : null,
              onTap: () => _pick<String?>('Who', [if (dispatchOn) (dispatcherWho, 'Dispatcher'), for (final e in employees) (e.id, e.name)], _who, (v) => _who = v),
            ),
            DetailRow(
              key: const ValueKey('new-session-model'),
              label: 'Model',
              value: _model == 'default' ? employeeDefault : ModelCatalog.friendly(_model),
              onTap: () => _pick('Model', [('default', employeeDefault), for (final m in models) (m, ModelCatalog.friendly(m))], _model, (v) => _model = v),
            ),
            DetailRow(
              key: const ValueKey('new-session-effort'),
              label: 'Effort',
              value: _effort == 'default' ? employeeDefault : ModelCatalog.effortLabel(_effort),
              onTap: () => _pick('Effort', [('default', employeeDefault), for (final (v, label, _) in ModelCatalog.efforts) (v, label)], _effort, (v) => _effort = v),
            ),
            DetailRow(
              key: const ValueKey('new-session-mode'),
              label: 'Mode',
              value: _mode == 'default' ? employeeDefault : AgentMode.fromJson(_mode).label,
              onTap: () => _pick('Mode', [('default', employeeDefault), for (final m in agentModes) (m.wire, m.label)], _mode, (v) => _mode = v),
            ),
          ]),
          Padding(
            padding: const EdgeInsets.fromLTRB(32, 12, 32, 0),
            child: Text(
              _tried && problem != null ? problem : note,
              key: const ValueKey('new-session-note'),
              style: TextStyle(fontSize: 13, color: _tried && problem != null ? Tokens.negative : Tokens.label2),
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
            child: FilledButton(onPressed: can ? _start : null, child: _busy ? const SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2)) : const Text('Start')),
          ),
        ],
      ),
    );
  }
}
