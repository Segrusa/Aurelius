import 'dart:io';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:image_picker/image_picker.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/mode_memory.dart';
import '../../data/views.dart';
import '../account/claude_login.dart' show ownSubscriptionNote;
import '../../data/run_config.dart';
import '../../core/widgets/ios.dart';
import 'variants_picker.dart';

/// What [NewTaskPage] returns.
/// `run`: mode, connection_id, model and effort of the task's conversation (unset: as the employee).
/// `projectId`: the project picked on the form (only when it shows the project field).
/// `files`: upload paths of the files added now; `removeFiles`: paths of a
/// schedule's own attachments taken off. `recurrence` and `paused` only for recurring tasks.
/// `variants`: try the task that many ways first (1 = as usual; feature `task_variants`).
typedef NewTask = ({
  String title,
  String goal,
  List<String> criteria,
  String? employeeId,
  String? projectId,
  Map<String, String> run,
  List<String> files,
  List<String> removeFiles,
  Recurrence? recurrence,
  bool paused,
  int variants,
});

/// A file on the form: already on a schedule ([path] is its place in the work
/// folder) or uploaded now ([path] is the upload).
class _Attached {
  const _Attached({required this.name, required this.byteLength, required this.path, this.existing = false});

  final String name;
  final int byteLength;
  final String path;
  final bool existing;
}

/// Files per task and their size (as messages).
const maxTaskFiles = 8;
const maxTaskFileBytes = 25 * 1024 * 1024;

/// Modes a task can run in, in the order the mode chip cycles: (wire value, label).
final taskModes = [for (final m in agentModes) (m.wire, m.label)];

/// The mode chip for a management lead that picks Auto or Plan itself.
const managerChoosesLabel = 'Manager chooses';

/// At most this many criteria (crates/domain task.rs `MAX_CRITERIA`).
const maxCriteria = 12;

/// Criteria typed one per line (or separated by `;`).
List<String> splitCriteria(String text) => text.split(RegExp(r'[\n;]')).map((c) => c.trim()).where((c) => c.isNotEmpty).toList();

/// A new task on a full screen: title, goal, criteria and the employee who
/// leads it in one place. Only the title is required; the goal defaults to it
/// on the PC. The lead picker lists the management team first, then the rest;
/// the single root is preselected. With [projects] (opened from
/// Home) the project is a field of the form too. With [recurring] it is the
/// form of a recurring task: a recurrence editor; [schedule] fills it to edit that one.
class NewTaskPage extends StatefulWidget {
  const NewTaskPage({
    super.key,
    required this.employees,
    this.projects,
    this.projectId,
    this.employeeId,
    this.title,
    this.recurring = false,
    this.schedule,
    this.submit,
  });

  final List<EmployeeSummary> employees;

  /// Sends the task from the form itself: false keeps the form open with
  /// what was typed (a B10 refusal, not sent). Without it the form just
  /// returns the task.
  final Future<bool> Function(BuildContext context, NewTask task)? submit;

  /// The projects to choose from; null when the project is already known.
  final List<ProjectSummary>? projects;

  /// Preselected project and lead, and a title typed elsewhere (Home's Give work).
  final String? projectId;
  final String? employeeId;
  final String? title;
  final bool recurring;
  final TaskScheduleView? schedule;

  static Future<NewTask?> open(
    BuildContext context,
    List<EmployeeSummary> employees, {
    List<ProjectSummary>? projects,
    String? projectId,
    String? employeeId,
    String? title,
    bool recurring = false,
    TaskScheduleView? schedule,
    Future<bool> Function(BuildContext context, NewTask task)? submit,
  }) =>
      Navigator.of(context).push<NewTask>(
        MaterialPageRoute(
          fullscreenDialog: true,
          builder: (_) => NewTaskPage(
            employees: employees,
            projects: projects,
            projectId: projectId,
            employeeId: employeeId,
            title: title,
            recurring: recurring || schedule != null,
            schedule: schedule,
            submit: submit,
          ),
        ),
      );

  @override
  State<NewTaskPage> createState() => _NewTaskPageState();
}

class _NewTaskPageState extends State<NewTaskPage> {
  late final _title = TextEditingController(text: widget.schedule?.title ?? widget.title);
  late final _goal = TextEditingController(text: widget.schedule?.goal);
  late final _criteria = TextEditingController(text: widget.schedule?.criteria.join('\n'));
  late String? _employee = widget.schedule?.employeeId ?? widget.employeeId ?? defaultAssignee(widget.employees);
  late String? _project = widget.projectId ?? _onlyProject;

  late final List<EmployeeGroup> _groups = pickerGroups(widget.employees);

  /// The only open project, preselected when there is just one.
  String? get _onlyProject {
    final open = (widget.projects ?? const <ProjectSummary>[]).where((p) => p.status == ProjectStatus.active).toList();
    return open.length == 1 ? open.single.id : null;
  }

  late final Map<String, String> _scheduleRun = widget.schedule?.run.choices ?? const {};
  late String? _mode = _scheduleRun['mode'];

  /// The user picked a mode here (or edits a schedule): the remembered one no longer applies.
  late bool _modeChosen = widget.schedule != null;
  late (String, String)? _model = switch (_scheduleRun) {
    final run when run['model'] != null => (run['connection_id'] ?? '', run['model']!),
    _ => null,
  }; // (connection id, model)
  late String? _effort = _scheduleRun['effort'];
  List<ConnectionView> _connections = const [];

  /// The run defaults, to show what "Employee default" resolves to.
  RunConfig? _config;

  /// What the chosen lead runs on without a choice here (employee, then project, then global).
  EmployeeRunView? get _default => switch ((_config, _employee)) {
        (final config?, final employee?) => config.employeeRun(employee, projectId: _project),
        _ => null,
      };

  /// The server's variants setting when it allows them; null hides the picker.
  VariantsSetting? _variantsSetting;
  int _variants = 1;

  late List<_Attached> _files = [for (final f in widget.schedule?.attachments ?? const []) _Attached(name: f.name, byteLength: f.byteLength, path: f.path, existing: true)];
  final _removed = <String>[];
  bool _uploading = false;
  bool _sending = false;

  late Recurrence _recurrence = widget.schedule?.recurrence ?? const RecurrenceDaily(atMinute: 9 * 60);
  late final _hours = TextEditingController(text: _recurrence is RecurrenceEveryHours ? '${(_recurrence as RecurrenceEveryHours).hours}' : '6');
  bool _paused = false;

  @override
  void initState() {
    super.initState();
    _loadConnections();
    if (!widget.recurring) _loadVariantsSetting();
    _loadManagementMode();
    _recallMode();
  }

  String? get _hostId {
    try {
      return context.read<AureliusRepository>().host.hostId;
    } on Object {
      return null; // no repository (tests)
    }
  }

  /// The mode this lead was last given is the default, until one is picked here.
  Future<void> _recallMode() async {
    final employee = _employee;
    final host = _hostId;
    if (_modeChosen || employee == null || host == null) return;
    final saved = await ModeMemory.read(host, employee);
    if (!mounted || _modeChosen || _employee != employee) return;
    setState(() => _mode = taskModes.any((m) => m.$1 == saved) ? saved : null);
  }

  /// The chip under the lead picker: tap goes to the next mode, then back to the default.
  void _cycleMode() {
    final values = [null, for (final (value, _) in taskModes) value];
    setState(() {
      _modeChosen = true;
      _mode = values[(values.indexOf(_mode) + 1) % values.length];
    });
  }

  String get _modeLabel => _mode == null ? _defaultModeLabel : {for (final (v, l) in taskModes) v: l}[_mode] ?? _mode!;

  /// Whether management picks Auto or Plan itself (the server has it and it is on).
  bool _managementChooses = false;

  Future<void> _loadManagementMode() async {
    try {
      final on = await context.read<AureliusRepository>().managementChoosesMode();
      if (mounted && on) setState(() => _managementChooses = true);
    } catch (_) {
      // No repository (tests) or offline: the usual default.
    }
  }

  /// What "no mode picked" is called: the lead chooses itself when it is management.
  String get _defaultModeLabel => _managementChooses && _employee != null && isManagement(widget.employees, _employee!) ? managerChoosesLabel : 'Employee default';

  /// Whether this server lets tasks be tried in several ways (feature `task_variants`, switched on).
  Future<void> _loadVariantsSetting() async {
    try {
      final setting = await context.read<AureliusRepository>().variantsSetting();
      if (mounted && setting != null && setting.enabled && setting.maxVariants > 1) setState(() => _variantsSetting = setting);
    } catch (_) {
      // No repository (tests) or offline: no variants.
    }
  }

  /// The models offered: every model of every usable connection.
  Future<void> _loadConnections() async {
    try {
      final snapshot = await context.read<AureliusRepository>().connections();
      if (mounted) {
        setState(() {
          _connections = snapshot.connections.where((c) => c.status != 'disconnected').toList();
          _config = RunConfig.of(snapshot);
        });
      }
    } catch (_) {
      // Offline or no repository: the task follows the employee.
    }
  }

  /// Photo, library or file, uploaded to the PC at once and sent with the task.
  Future<void> _attach() async {
    if (_files.length >= maxTaskFiles) return toast(context, 'At most $maxTaskFiles files per task.');
    final choice = await showModalBottomSheet<String>(
      context: context,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(leading: const Icon(Icons.photo_camera_outlined), title: const Text('Take a photo'), onTap: () => Navigator.pop(sheet, 'camera')),
            ListTile(leading: const Icon(Icons.photo_library_outlined), title: const Text('Photo library'), onTap: () => Navigator.pop(sheet, 'gallery')),
            ListTile(leading: const Icon(Icons.description_outlined), title: const Text('File'), onTap: () => Navigator.pop(sheet, 'file')),
          ],
        ),
      ),
    );
    if (choice == null || !mounted) return;
    final room = maxTaskFiles - _files.length;
    final List<(File, String)> picked;
    if (choice == 'file') {
      picked = [for (final f in await FilePicker.pickFiles()) if (f.path != null) (File(f.path!), f.name)];
    } else if (choice == 'gallery') {
      picked = [for (final i in await ImagePicker().pickMultiImage(imageQuality: 85, maxWidth: 2400, limit: room)) (File(i.path), i.name)];
    } else {
      final image = await ImagePicker().pickImage(source: ImageSource.camera, imageQuality: 85, maxWidth: 2400);
      picked = [if (image != null) (File(image.path), image.name)];
    }
    if (picked.isEmpty || !mounted) return;
    final repository = context.read<AureliusRepository>();
    setState(() => _uploading = true);
    try {
      // Each file over 25 MB, or beyond the task's limit, is left out and
      // named in one notice; the rest is uploaded.
      final tooLarge = <String>[];
      final fits = <(File, String)>[];
      for (final (file, name) in picked) {
        if (await file.length() > maxTaskFileBytes) {
          tooLarge.add(name);
        } else {
          fits.add((file, name));
        }
      }
      final kept = fits.take(maxTaskFiles - _files.length).toList();
      final notice = [
        if (tooLarge.isNotEmpty) '${_names(tooLarge)} ${tooLarge.length == 1 ? 'is' : 'are'} larger than 25 MB and ${tooLarge.length == 1 ? 'was' : 'were'} not added.',
        if (kept.length < fits.length) 'At most $maxTaskFiles files per task; ${fits.length - kept.length} not added.',
      ].join(' ');
      if (notice.isNotEmpty && mounted) toast(context, notice);
      for (final (file, name) in kept) {
        try {
          final uploaded = await repository.upload(file, name);
          if (mounted) setState(() => _files = [..._files, _Attached(name: uploaded.name, byteLength: uploaded.byteLength, path: uploaded.path)]);
        } catch (e) {
          if (mounted) toast(context, '$name: $e');
        }
      }
    } finally {
      if (mounted) setState(() => _uploading = false);
    }
  }

  static String _names(List<String> names) => names.length == 1 ? names.single : '${names.take(names.length - 1).join(', ')} and ${names.last}';

  void _removeFile(_Attached f) => setState(() {
        _files = _files.where((x) => x != f).toList();
        if (f.existing) _removed.add(f.path);
      });

  /// "Employee default (Opus 5.5 · High)" for the model, "Employee default (High)"
  /// for the effort: what the lead runs on when nothing is chosen here. Never
  /// changes that default; it is set on the employee page.
  String _defaultLabel({required bool model}) {
    final run = _default;
    if (run == null) return 'Employee default';
    final effortShown = run.connection?.supportsEffort ?? true;
    final effort = effortShown && run.effort.value != null ? ModelCatalog.effortLabel(run.effort.value) : null;
    final value = model ? [run.model.value == null ? 'Default' : ModelCatalog.friendly(run.model.value!), ?effort].join(' · ') : effort ?? 'Default';
    return 'Employee default ($value)';
  }

  Future<void> _pickTime() async {
    final at = _atMinute;
    final picked = await showTimePicker(context: context, initialTime: TimeOfDay(hour: at ~/ 60, minute: at % 60));
    if (picked != null) _setAtMinute(picked.hour * 60 + picked.minute);
  }

  int get _atMinute => switch (_recurrence) {
        RecurrenceDaily(:final atMinute) => atMinute,
        RecurrenceWeekly(:final atMinute) => atMinute,
        _ => 9 * 60,
      };

  void _setAtMinute(int minute) => setState(() => _recurrence = switch (_recurrence) {
        RecurrenceWeekly(:final weekdays) => RecurrenceWeekly(weekdays: weekdays, atMinute: minute),
        _ => RecurrenceDaily(atMinute: minute),
      });

  void _setKind(String kind) => setState(() {
        _recurrence = switch (kind) {
          'every_hours' => RecurrenceEveryHours(hours: int.tryParse(_hours.text) ?? 6),
          'weekly' => RecurrenceWeekly(weekdays: const [0], atMinute: _atMinute),
          _ => RecurrenceDaily(atMinute: _atMinute),
        };
      });

  void _toggleDay(int day) {
    final current = _recurrence as RecurrenceWeekly;
    final days = (current.weekdays.contains(day) ? current.weekdays.where((d) => d != day).toList() : [...current.weekdays, day])..sort();
    setState(() => _recurrence = RecurrenceWeekly(weekdays: days, atMinute: current.atMinute));
  }

  bool get _recurrenceValid => switch (_recurrence) {
        RecurrenceEveryHours(:final hours) => hours >= 1 && hours <= 8760,
        RecurrenceWeekly(:final weekdays) => weekdays.isNotEmpty,
        _ => true,
      };

  Map<String, String> get _run => {
        'mode': ?_mode,
        if (_model != null) ...{'connection_id': _model!.$1, 'model': _model!.$2},
        'effort': ?_effort,
      };

  List<String> get _criteriaList => splitCriteria(_criteria.text);

  /// Whether a schedule being edited already sets something under Advanced; then it starts open.
  bool get _advancedSet => _criteria.text.trim().isNotEmpty || _mode != null || _model != null || _effort != null;

  /// One line for the folded Advanced section: "Employee default · Ask · High · 2 criteria".
  String get _summary {
    final count = _criteriaList.length;
    final parts = [
      if (_model == null && _effort == null && _mode == null) _defaultModeLabel,
      if (_model case (_, final model)?) ModelCatalog.friendly(model),
      if (_mode case final mode?) {for (final (v, l) in taskModes) v: l}[mode] ?? mode,
      if (_effort case final effort?) ModelCatalog.effortLabel(effort),
      if (_variantsSetting != null && _variants > 1) '$_variants ways',
      if (count > 0) '$count ${count == 1 ? 'criterion' : 'criteria'}',
    ];
    return parts.join(' · ');
  }

  bool get _valid =>
      _title.text.trim().isNotEmpty &&
      _criteriaList.length <= maxCriteria &&
      !_uploading &&
      !_sending &&
      (widget.projects == null || _project != null) &&
      (!widget.recurring || (_employee != null && _recurrenceValid));

  Future<void> _create() async {
    if (!_valid) return;
    // The next task for this lead starts in the mode used now (none: its default).
    final host = _hostId;
    if (host != null && _employee != null && !widget.recurring && widget.schedule == null) ModeMemory.write(host, _employee!, _mode);
    final NewTask task = (
      title: _title.text.trim(),
      goal: _goal.text.trim(),
      criteria: _criteriaList,
      employeeId: _employee,
      projectId: _project,
      run: _run,
      files: [for (final f in _files) if (!f.existing) f.path],
      removeFiles: _removed,
      recurrence: widget.recurring ? _recurrence : null,
      paused: _paused,
      variants: _variantsSetting == null ? 1 : _variants,
    );
    if (widget.submit case final submit?) {
      setState(() => _sending = true);
      final sent = await submit(context, task);
      if (!mounted) return;
      setState(() => _sending = false);
      // Not sent: everything typed stays (QA2 m8).
      if (!sent) return;
    }
    Navigator.pop<NewTask>(context, task);
  }

  @override
  void dispose() {
    _title.dispose();
    _goal.dispose();
    _criteria.dispose();
    _hours.dispose();
    super.dispose();
  }

  List<Widget> _recurrenceEditor() {
    final kind = switch (_recurrence) {
      RecurrenceEveryHours() => 'every_hours',
      RecurrenceWeekly() => 'weekly',
      _ => 'daily',
    };
    final at = _atMinute;
    final time = TimeOfDay(hour: at ~/ 60, minute: at % 60).format(context);
    return [
      const Eyebrow('Repeats', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
      // B1-2: a schedule runs on the login of whoever makes it or sets it again.
      Padding(
        padding: const EdgeInsets.only(bottom: 10),
        child: Text(ownSubscriptionNote, key: const ValueKey('schedule-subscription'), style: TextStyle(fontSize: 13, color: Tokens.label2)),
      ),
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          for (final (value, label) in const [('every_hours', 'Every N hours'), ('daily', 'Daily'), ('weekly', 'Weekly')])
            ChoiceChip(label: Text(label), selected: kind == value, onSelected: (_) => _setKind(value)),
        ],
      ),
      const SizedBox(height: 12),
      if (_recurrence case RecurrenceEveryHours(:final hours))
        TextField(
          controller: _hours,
          keyboardType: TextInputType.number,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
          decoration: InputDecoration(labelText: 'Every … hours', helperText: 'From 1 hour up to a year (8760)', errorText: hours >= 1 && hours <= 8760 ? null : 'Between 1 and 8760'),
          onChanged: (v) => setState(() => _recurrence = RecurrenceEveryHours(hours: int.tryParse(v) ?? 0)),
        ),
      if (_recurrence case RecurrenceWeekly(:final weekdays)) ...[
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            for (final (i, d) in weekdayNames.indexed) FilterChip(label: Text(d), selected: weekdays.contains(i), onSelected: (_) => _toggleDay(i)),
          ],
        ),
        if (weekdays.isEmpty)
          Padding(padding: const EdgeInsets.only(top: 6), child: Text('Pick at least one day', style: TextStyle(color: Theme.of(context).colorScheme.error, fontSize: 12))),
        const SizedBox(height: 8),
      ],
      if (kind != 'every_hours') OutlinedButton.icon(onPressed: _pickTime, icon: const Icon(Icons.schedule, size: 18), label: Text('At $time')),
      Padding(
        padding: const EdgeInsets.only(top: 8),
        child: Text('Server time. If the previous task is still open when it is due, that run is skipped.', style: TextStyle(fontSize: 12, color: Theme.of(context).hintColor)),
      ),
      if (widget.schedule == null) SwitchListTile(contentPadding: EdgeInsets.zero, title: const Text('Start paused'), value: _paused, onChanged: (v) => setState(() => _paused = v)),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final count = _criteriaList.length;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): _create,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): _create,
      },
      child: Scaffold(
        appBar: GlassAppBar(
          title: Text(widget.schedule != null ? 'Edit recurring task' : (widget.recurring ? 'New recurring task' : 'New task')),
          leading: IconButton(tooltip: 'Cancel', icon: const Icon(Icons.close), onPressed: () => Navigator.pop(context)),
          actions: [
            Padding(
              padding: const EdgeInsets.only(right: 12),
              child: Tooltip(message: 'Create (⌘↩)', child: FilledButton(onPressed: _valid ? _create : null, child: Text(widget.schedule != null ? 'Save' : 'Create'))),
            ),
          ],
        ),
        body: SafeArea(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
            keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
            children: [
              TextField(
                controller: _title,
                autofocus: widget.schedule == null,
                textCapitalization: TextCapitalization.sentences,
                textInputAction: TextInputAction.next,
                maxLength: 160,
                decoration: const InputDecoration(labelText: 'Title', hintText: 'What should be done?', counterText: ''),
                onChanged: (_) => setState(() {}),
              ),
              const SizedBox(height: 14),
              TextField(
                controller: _goal,
                minLines: 4,
                maxLines: 12,
                keyboardType: TextInputType.multiline,
                textInputAction: TextInputAction.newline,
                textCapitalization: TextCapitalization.sentences,
                decoration: const InputDecoration(labelText: 'Goal (optional)', hintText: 'Context, the outcome you want, constraints', alignLabelWithHint: true),
              ),
              if (widget.projects case final projects?) ...[
                const Eyebrow('Project', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
                if (projects.isEmpty) Text('Create a project first; work is done in a project.', style: TextStyle(color: Theme.of(context).hintColor)),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    for (final p in projects.where((p) => p.status != ProjectStatus.archived))
                      ChoiceChip(
                        key: ValueKey('new-task-project-${p.id}'),
                        label: Text(p.status == ProjectStatus.paused ? '${p.name} (paused)' : p.name),
                        selected: _project == p.id,
                        onSelected: (_) => setState(() => _project = p.id),
                      ),
                  ],
                ),
              ],
              if (_groups.isNotEmpty) ...[
                const Eyebrow('Who leads this task?', padding: EdgeInsets.fromLTRB(0, 22, 0, 4)),
                for (final g in _groups) ...[
                  Padding(
                    padding: const EdgeInsets.only(top: 8, bottom: 6),
                    child: Text(g.title, style: TextStyle(fontSize: 13, fontWeight: FontWeight.w600, color: Theme.of(context).hintColor)),
                  ),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      for (final m in g.employees)
                        ChoiceChip(
                          key: ValueKey('lead-${m.id}'),
                          avatar: Monogram(m.monogram, size: 22),
                          label: Text(m.status == LifecycleStatus.paused ? '${m.name} (paused)' : m.name),
                          tooltip: m.roleLine,
                          selected: _employee == m.id,
                          onSelected: (_) {
                            setState(() => _employee = m.id);
                            _recallMode();
                          },
                        ),
                    ],
                  ),
                ],
              ],
              if (_groups.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: Align(
                    alignment: Alignment.centerLeft,
                    child: ActionChip(
                      key: const ValueKey('new-task-mode-chip'),
                      avatar: Icon(Icons.tune, size: 16, color: _mode == 'auto' ? Tokens.warning : null),
                      label: Text('Mode: $_modeLabel'),
                      tooltip: 'Tap for the next mode',
                      onPressed: _cycleMode,
                    ),
                  ),
                ),
              if (widget.recurring) ..._recurrenceEditor(),
              const Eyebrow('Attachments', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  for (final f in _files)
                    InputChip(
                      avatar: const Icon(Icons.attach_file, size: 18),
                      label: Text('${f.name} · ${Fmt.bytes(f.byteLength)}', overflow: TextOverflow.ellipsis),
                      onDeleted: () => _removeFile(f),
                    ),
                  if (_uploading) const SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)),
                  ActionChip(avatar: const Icon(Icons.add, size: 18), label: const Text('Photo or file'), onPressed: _uploading ? null : _attach),
                ],
              ),
              const SizedBox(height: 14),
              ExpansionTile(
                key: const ValueKey('new-task-advanced'),
                initiallyExpanded: _advancedSet,
                maintainState: true,
                tilePadding: EdgeInsets.zero,
                childrenPadding: EdgeInsets.zero,
                expandedCrossAxisAlignment: CrossAxisAlignment.stretch,
                shape: const Border(),
                collapsedShape: const Border(),
                title: const Text('Advanced'),
                subtitle: Text(_summary, key: const ValueKey('new-task-summary')),
                children: [
                  const SizedBox(height: 8),
                  TextField(
                    controller: _criteria,
                    minLines: 3,
                    maxLines: 10,
                    textCapitalization: TextCapitalization.sentences,
                    decoration: InputDecoration(
                      labelText: 'Acceptance criteria (optional)',
                      hintText: 'One per line',
                      alignLabelWithHint: true,
                      helperText: count == 0 ? 'One per line. The reviewer checks each one.' : '$count of at most $maxCriteria',
                      errorText: count > maxCriteria ? 'At most $maxCriteria criteria' : null,
                    ),
                    onChanged: (_) => setState(() {}),
                  ),
                  const Eyebrow('Mode', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      ChoiceChip(label: Text(_defaultModeLabel), selected: _mode == null, onSelected: (_) => setState(() {
                        _modeChosen = true;
                        _mode = null;
                      })),
                      for (final (value, label) in taskModes)
                        ChoiceChip(
                          label: Text(label),
                          selected: _mode == value,
                          onSelected: (_) => setState(() {
                            _modeChosen = true;
                            _mode = value;
                          }),
                        ),
                    ],
                  ),
                  if (_connections.isNotEmpty) ...[
                    const Eyebrow('Model for this task', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
                    Wrap(
                      spacing: 8,
                      runSpacing: 8,
                      children: [
                        ChoiceChip(
                          key: const ValueKey('new-task-model-default'),
                          label: Text(_defaultLabel(model: true)),
                          selected: _model == null,
                          onSelected: (_) => setState(() => _model = null),
                        ),
                        for (final c in _connections)
                          for (final m in c.models.isEmpty ? [?c.defaultModel] : c.models)
                            ChoiceChip(
                              label: Text(_connections.length > 1 ? '${ModelCatalog.friendly(m)} · ${c.displayName}' : ModelCatalog.friendly(m)),
                              selected: _model == (c.id, m),
                              onSelected: (_) => setState(() => _model = (c.id, m)),
                            ),
                      ],
                    ),
                  ],
                  if (_variantsSetting != null) ...variantsPicker(context, max: _variantsSetting!.maxVariants, value: _variants, onChanged: (n) => setState(() => _variants = n)),
                  const Eyebrow('Effort for this task', padding: EdgeInsets.fromLTRB(0, 22, 0, 10)),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      ChoiceChip(
                        key: const ValueKey('new-task-effort-default'),
                        label: Text(_defaultLabel(model: false)),
                        selected: _effort == null,
                        onSelected: (_) => setState(() => _effort = null),
                      ),
                      for (final (value, label, _) in ModelCatalog.efforts) ChoiceChip(label: Text(label), selected: _effort == value, onSelected: (_) => setState(() => _effort = value)),
                    ],
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
