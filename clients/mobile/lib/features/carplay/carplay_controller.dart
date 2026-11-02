import 'dart:async';

import 'package:shared_preferences/shared_preferences.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/company.dart';
import '../../data/views.dart';
import 'carplay_screen.dart';
import 'carplay_selection.dart';
import 'speech_text.dart';

/// Where what the user says goes: the chosen employee's chat with the Board,
/// or the conversation of a task that waits for the user.
class _Target {
  const _Target(this.employeeId, this.employeeName, {this.conversationId, this.task});

  final String employeeId;
  final String employeeName;

  /// Null: the employee's own chat.
  final String? conversationId;
  final TaskView? task;
}

/// CarPlay opens on its list: talk to the chosen employee (the management
/// team's single root unless another was picked; what was said goes over the
/// normal writes, the answer is read aloud), log a task or an idea by voice,
/// pick another employee or answer a task that needs the user.
class CarPlayController {
  CarPlayController({required this.repository, required this.screen, required this.prefs});

  final AureliusRepository repository;
  final CarScreen screen;
  final SharedPreferences prefs;

  /// How long CarPlay waits for an answer before leaving it to the phone.
  static const answerTimeout = Duration(minutes: 10);

  /// How long the first connection may take before CarPlay says it is offline.
  static const connectWait = Duration(seconds: 8);

  final List<StreamSubscription<dynamic>> _subs = [];
  List<ProjectSummary> _projects = const [];
  List<EmployeeSummary> _employees = const [];
  List<TaskView> _tasks = const [];
  List<TaskView> _needsYou = const [];
  _Target? _target;
  bool _loaded = false;

  /// Each voice exchange has a number; a newer one (or stop) ends the older.
  int _turn = 0;
  bool _busy = false;
  bool _voiceOpen = false;
  VoiceState? _shown;
  Timer? _refresh;

  /// The picked employee; employees kept their employee ids, so the old key stays.
  String get _savedKey => 'carplay.employee.${repository.host.hostId}';

  Future<void> start() async {
    _subs.add(screen.events.listen(_onEvent));
    _subs.add(repository.link.listen(_onLink));
    _subs.add(repository.companyChanges.listen((_) => _scheduleRefresh()));
    await _showLists();
    if (await _waitOnline()) await _load();
  }

  Future<void> dispose() async {
    _turn++;
    _refresh?.cancel();
    for (final s in _subs) {
      await s.cancel();
    }
  }

  // MARK: Connection

  Future<bool> _waitOnline() async {
    if (repository.linkState.isOnline) return true;
    try {
      await repository.link.firstWhere((s) => s.isOnline).timeout(connectWait);
      return true;
    } on TimeoutException {
      return false;
    } on StateError {
      return false;
    }
  }

  String _offlineHint() => switch (repository.linkState.status) {
        LinkStatus.revoked => 'This iPhone is no longer paired; pair it again on the phone.',
        LinkStatus.incompatible => differentVersions,
        _ => 'Is the PC on, with Aurelius and Tailscale running?',
      };

  // MARK: Data

  Future<void> _load() async {
    try {
      final results = await Future.wait([repository.projects(), repository.employees(), repository.tasks()]);
      _projects = results[0] as List<ProjectSummary>;
      _employees = results[1] as List<EmployeeSummary>;
      _tasks = results[2] as List<TaskView>;
      _needsYou = needsYouTasks(_tasks, others: await repository.mainTasksOfOthers(_tasks));
      _loaded = true;
    } on Exception {
      // Offline or the PC did not answer: keep what we had.
    }
    if (_target?.task == null) {
      final employee = chooseEmployee(_employees, _target?.employeeId ?? prefs.getString(_savedKey));
      _target = employee == null ? null : _Target(employee.id, employee.name);
    }
    await _showLists();
  }

  void _scheduleRefresh() {
    _refresh?.cancel();
    _refresh = Timer(const Duration(seconds: 2), () => unawaited(_load()));
  }

  /// The voice template's state; remembered so coming back online can clear "offline".
  Future<void> _voice(VoiceState state) {
    _shown = state;
    return screen.voice(state);
  }

  void _onLink(LinkState state) {
    if (state.isOnline) {
      _scheduleRefresh();
      if (_voiceOpen && _shown == VoiceState.offline) unawaited(_voice(VoiceState.ready));
    } else if (!_busy) {
      unawaited(_showLists());
      if (_voiceOpen && state.status != LinkStatus.connecting) unawaited(_voice(VoiceState.offline));
    }
  }

  // MARK: Lists

  Future<void> _showLists() async {
    final link = repository.linkState;
    final target = _target;
    CarItem employee(EmployeeSummary m) => CarItem(id: 'employee:${m.id}', text: m.name, detail: m.roleLine.isEmpty ? null : m.roleLine);

    final project = _project();
    final talk = !link.isOnline
        ? CarItem(id: 'retry', text: 'No connection', detail: link.message ?? 'Tap to try again')
        : target == null
            ? CarItem(id: 'reload', text: _loaded ? 'No employees yet' : 'Loading…')
            : CarItem(id: 'talk', text: 'Talk to ${target.employeeName}', detail: 'Tap, then speak');
    final team = managementTeam(_employees);
    final teamIds = {for (final m in team) m.id};
    final recent = recentEmployees(_employees).where((m) => !teamIds.contains(m.id)).toList();
    final groups = pickerGroups(_employees).where((g) => g.title != 'Management team').toList();
    await screen.lists([
      CarSection(items: [
        talk,
        if (link.isOnline && project != null) ...[
          CarItem(id: 'log:task', text: 'Log a task', detail: 'In ${project.name}, as a draft'),
          CarItem(id: 'log:idea', text: 'Log an idea', detail: 'In ${project.name}'),
        ],
      ]),
      if (_needsYou.isNotEmpty)
        CarSection(header: 'Needs you · ${_needsYou.length}', items: [
          const CarItem(id: 'needs', text: 'Hear what needs you'),
          for (final t in _needsYou.take(4)) CarItem(id: 'task:${t.id}', text: t.title, detail: [t.statusLabel, ?t.employeeName].join(' · ')),
        ]),
      if (team.isNotEmpty) CarSection(header: 'Management team', items: [for (final m in team.take(carListLimit)) employee(m)]),
      if (recent.isNotEmpty) CarSection(header: 'Recent', items: [for (final m in recent.take(4)) employee(m)]),
      if (recent.length > 4 && groups.isNotEmpty)
        CarSection(header: 'Everyone', items: [
          for (final g in groups) CarItem(text: g.title, children: [for (final m in g.employees.take(carListLimit)) employee(m)]),
        ]),
    ]);
  }

  // MARK: Events

  Future<void> _onEvent(CarEvent event) async {
    switch (event.kind) {
      case CarEventKind.select:
        await _select(event.id ?? '');
      case CarEventKind.mic:
        _voiceOpen = true;
        await _talk(++_turn);
      case CarEventKind.stop:
      case CarEventKind.interrupted:
        _turn++;
        _busy = false;
        await _voice(repository.linkState.isOnline ? VoiceState.ready : VoiceState.offline);
      case CarEventKind.voiceClosed:
        _turn++;
        _busy = false;
        _voiceOpen = false;
        _shown = null;
    }
  }

  Future<void> _select(String id) async {
    final turn = ++_turn;
    if (id == 'retry' || id == 'reload') {
      repository.connection.wake();
      await _load();
      return;
    }
    _voiceOpen = true;
    if (id == 'talk') {
      _backToEmployee();
      await _talk(turn);
    } else if (id == 'log:task' || id == 'log:idea') {
      await _log(task: id == 'log:task', turn: turn);
    } else if (id == 'needs') {
      await _say(needsYouSummary(_needsYou), turn);
      if (turn == _turn) await _voice(VoiceState.ready);
    } else if (id.startsWith('employee:')) {
      final employee = _employees.where((m) => m.id == id.substring(9)).firstOrNull;
      if (employee == null) return;
      await prefs.setString(_savedKey, employee.id);
      _target = _Target(employee.id, employee.name);
      unawaited(screen.popToRoot());
      unawaited(_showLists());
      if (!await _say('Talking to ${employee.name}. Go ahead.', turn)) return;
      await _talk(turn);
    } else if (id.startsWith('task:')) {
      final task = _needsYou.where((t) => t.id == id.substring(5)).firstOrNull;
      if (task == null) return;
      await _task(task, turn);
    }
  }

  /// Reads out a waiting task with its latest message, then takes the answer.
  Future<void> _task(TaskView task, int turn) async {
    final parts = [taskIntro(task)];
    final conversationId = task.conversationId;
    if (conversationId != null) {
      try {
        final latest = (await repository.messages(conversationId)).messages.where((m) => !m.isUser && m.text.trim().isNotEmpty).lastOrNull;
        if (latest != null) parts.add(spokenAnswer(latest.text, limit: 400));
      } on Exception {
        // Read the title alone.
      }
    }
    parts.add(taskPrompt(task));
    if (turn != _turn) return;
    if (!answerableByVoice(task)) {
      await _say(parts.join(' '), turn);
      if (turn == _turn) await _voice(VoiceState.ready);
      return;
    }
    _target = _Target(task.employeeId!, task.employeeName ?? 'the employee', conversationId: conversationId, task: task);
    if (!await _say(parts.join(' '), turn)) return;
    await _talk(turn);
  }

  /// After a task's answer, talking goes back to the chosen employee.
  void _backToEmployee() {
    if (_target?.task == null) return;
    final employee = chooseEmployee(_employees, prefs.getString(_savedKey));
    _target = employee == null ? null : _Target(employee.id, employee.name);
  }

  // MARK: Talking

  /// Says [text]; false when it was stopped or a newer exchange started.
  Future<bool> _say(String text, int turn) async {
    if (turn != _turn) return false;
    final finished = await screen.speak(text);
    return finished && turn == _turn;
  }

  /// Listen, send what was said and read the answer aloud.
  Future<void> _talk(int turn) async {
    final target = _target;
    if (target == null) {
      await _say('Pick an employee in the list first.', turn);
      return;
    }
    if (!repository.linkState.isOnline) {
      await _say('No connection to ${repository.host.name}, so I cannot send anything now. ${_offlineHint()}', turn);
      if (turn == _turn) await _voice(VoiceState.offline);
      return;
    }
    _busy = true;
    try {
      final said = await _hear(turn);
      if (said == null) return;
      await _send(target, said, turn);
    } finally {
      if (turn == _turn) {
        _busy = false;
        if (target.task != null) _backToEmployee();
      }
    }
  }

  /// What the user says; null when nothing was said, it failed (and was
  /// told) or a newer exchange started.
  Future<String?> _hear(int turn) async {
    final String? said;
    try {
      said = await screen.listen();
    } on CarVoiceException catch (e) {
      await _say(switch (e.code) {
        'denied' => 'Aurelius may not use the microphone or speech recognition. Allow both in the Settings app on your iPhone, when you are parked.',
        'unavailable' => 'Speech recognition is not available right now.',
        _ => 'The microphone could not start.',
      }, turn);
      return null;
    }
    if (turn != _turn) return null;
    if (said == null) await _voice(VoiceState.ready);
    return said;
  }

  /// Where dictated tasks and ideas go: the task's project, else the project
  /// the chosen employee worked in last (or the newest active project).
  ProjectSummary? _project() {
    final target = _target;
    if (target?.task case final task?) return _projects.where((p) => p.id == task.projectId).firstOrNull;
    return loggingProject(_projects, _tasks, target?.employeeId);
  }

  /// Logs a dictated task (a draft without a lead, so nothing starts) or
  /// idea in [_project].
  Future<void> _log({required bool task, required int turn}) async {
    final project = _project();
    if (project == null) {
      await _say('There is no active project to log it in.', turn);
      return;
    }
    if (!repository.linkState.isOnline) {
      await _say('No connection to ${repository.host.name}, so I cannot log anything now. ${_offlineHint()}', turn);
      if (turn == _turn) await _voice(VoiceState.offline);
      return;
    }
    if (!await _say(task ? 'What is the task, for ${project.name}?' : 'What is the idea, for ${project.name}?', turn)) return;
    _busy = true;
    try {
      final said = await _hear(turn);
      if (said == null || said.trim().isEmpty) return;
      final note = dictatedNote(said);
      await _voice(VoiceState.thinking);
      try {
        await repository.write(task
            ? Commands.createTask(project.id, note.title, note.body, const [])
            : Commands.addIdea(project.id, note.title, note.body.length > note.title.length + 1 ? note.body : null));
      } on NotSentException {
        await _say('Not logged: the PC was not reachable.', turn);
        return;
      } on ProtocolException catch (e) {
        await _say('Not logged. ${e.message}', turn);
        return;
      }
      if (turn != _turn) return;
      await _say(task ? 'Task logged as a draft in ${project.name}: ${note.title}.' : 'Idea logged in ${project.name}: ${note.title}.', turn);
      if (turn == _turn) await _voice(VoiceState.ready);
    } finally {
      if (turn == _turn) _busy = false;
    }
  }

  Future<void> _send(_Target target, String text, int turn) async {
    await _voice(VoiceState.thinking);
    final task = target.task;
    // A task's conversation, else the employee's own chat.
    final conversationId = target.conversationId ?? _employees.where((m) => m.id == target.employeeId).firstOrNull?.conversationId;
    if (conversationId == null) {
      await _say('Pick an employee in the list first.', turn);
      return;
    }
    final after = await _lastOrdinal(conversationId);
    final Receipt receipt;
    try {
      receipt = await repository.write(Commands.sendEmployeeMessage(target.employeeId, text, conversationId: conversationId));
    } on NotSentException {
      await _say('Not sent: the PC was not reachable.', turn);
      return;
    } on ProtocolException catch (e) {
      await _say('Not sent. ${e.message}', turn);
      return;
    }
    if (turn != _turn) return;
    final into = switch (receipt.command) {
      CommandResultEmployeeMessageSent(:final conversationId) => conversationId,
      _ => conversationId,
    };
    if (after == null) {
      await _say('Sent to ${target.employeeName}. The answer will be on your phone.', turn);
      return;
    }
    await _voice(VoiceState.thinking);
    final answer = await _answer(into, after, text, turn);
    if (turn != _turn) return;
    if (answer == null) {
      await _say('${target.employeeName} is still working on it${task == null ? '' : ' in the task'}. The answer will be on your phone.', turn);
    } else {
      final spoken = spokenAnswer(answer);
      await _say(spoken.isEmpty ? '${target.employeeName} answered without words; see your phone.' : spoken, turn);
    }
    if (turn == _turn) await _voice(VoiceState.ready);
  }

  Future<int?> _lastOrdinal(String conversationId) async {
    try {
      final messages = (await repository.messages(conversationId)).messages;
      return messages.fold<int>(-1, (last, m) => m.ordinal > last ? m.ordinal : last);
    } on Exception {
      return null;
    }
  }

  /// The answer to what was just sent: the replies after [after] once none of
  /// them is still being written (and a moment passed for a follow-up).
  /// Null after [answerTimeout] or when a newer exchange started.
  Future<String?> _answer(String conversationId, int after, String sent, int turn) async {
    final done = Completer<String?>();
    Timer? settle;
    Timer? debounce;
    var checking = false;

    Future<void> check() async {
      if (checking || done.isCompleted) return;
      if (turn != _turn) {
        done.complete(null);
        return;
      }
      checking = true;
      try {
        final messages = (await repository.messages(conversationId)).messages.where((m) => m.ordinal > after).toList();
        final mine = messages.where((m) => m.isUser && m.text.trim() == sent.trim()).lastOrNull;
        var replies = messages.where((m) => !m.isUser).toList();
        final direct = mine == null ? const <MessageView>[] : replies.where((m) => m.replyTo == mine.id).toList();
        if (direct.isNotEmpty) replies = direct;
        settle?.cancel();
        if (replies.isNotEmpty && !replies.any((m) => m.isStreaming)) {
          settle = Timer(const Duration(seconds: 2), () {
            if (!done.isCompleted) done.complete(replies.map((m) => m.text.trim()).where((t) => t.isNotEmpty).join('\n\n'));
          });
        }
      } on Exception {
        // Offline for a moment: the next change or poll tries again.
      } finally {
        checking = false;
      }
    }

    void soon() {
      debounce?.cancel();
      debounce = Timer(const Duration(milliseconds: 700), () => unawaited(check()));
    }

    final changes = repository.chatChanges.listen((_) => soon());
    final online = repository.online.listen((_) => soon());
    final poll = Timer.periodic(const Duration(seconds: 6), (_) => unawaited(check()));
    unawaited(check());
    try {
      return await done.future.timeout(answerTimeout, onTimeout: () => null);
    } finally {
      settle?.cancel();
      debounce?.cancel();
      poll.cancel();
      await changes.cancel();
      await online.cancel();
    }
  }
}
