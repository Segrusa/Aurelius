import 'dart:math';

import '../../data/company.dart';
import '../../data/views.dart';

/// What CarPlay lists and who it talks to: recent first, short, and only
/// what can still be acted on.

/// CarPlay shows short lists; the car may allow fewer.
const carListLimit = 12;

/// When an employee was last active: its newest conversation.
int employeeActivity(EmployeeSummary employee) => employee.conversations.fold(0, (latest, c) => max(latest, c.updatedAtMs));

/// Active employees, most recently active first.
List<EmployeeSummary> recentEmployees(List<EmployeeSummary> employees) {
  final list = employees.where((m) => m.status == LifecycleStatus.active).toList()
    ..sort((a, b) {
      final byActivity = employeeActivity(b).compareTo(employeeActivity(a));
      return byActivity != 0 ? byActivity : a.name.toLowerCase().compareTo(b.name.toLowerCase());
    });
  return list;
}

/// The employee CarPlay talks to: the one picked before ([savedId]) while it
/// is still active, otherwise the single root of the company, otherwise the
/// most recently active one.
EmployeeSummary? chooseEmployee(List<EmployeeSummary> employees, String? savedId) {
  final recent = recentEmployees(employees);
  final root = defaultAssignee(employees);
  return recent.where((m) => m.id == savedId).firstOrNull ?? recent.where((m) => m.id == root).firstOrNull ?? recent.firstOrNull;
}

/// Where a dictated task or idea goes: the active project [employeeId] most
/// recently worked in, else the most recently created active project.
ProjectSummary? loggingProject(List<ProjectSummary> projects, List<TaskView> tasks, String? employeeId) {
  final active = projects.where((p) => p.status == ProjectStatus.active).toList();
  if (employeeId != null) {
    for (final id in projectsOf(employeeId, tasks)) {
      if (active.where((p) => p.id == id).firstOrNull case final p?) return p;
    }
  }
  return (active..sort((a, b) => b.createdAtMs.compareTo(a.createdAtMs))).firstOrNull;
}

/// Tasks waiting for the user, most recently changed first.
List<TaskView> needsYouTasks(List<TaskView> tasks) => tasks.where((t) => t.needsYou && !t.isDone).toList()..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));

/// A plan or an approval is decided on the phone (Inbox); input, escalations
/// and the like can be answered by voice in the task's conversation.
bool answerableByVoice(TaskView task) => task.conversationId != null && task.employeeId != null && task.pendingApprovals == 0;

/// "Two tasks need you: Fix login, for Max; and Release notes, for Dean."
String needsYouSummary(List<TaskView> tasks, {int read = 3}) {
  if (tasks.isEmpty) return 'Nothing needs you right now.';
  String one(TaskView t) => t.employeeName == null ? _plain(t.title) : '${_plain(t.title)}, for ${t.employeeName}';
  final count = needsYouCount(tasks.length);
  final named = tasks.take(read).map(one).toList();
  final more = tasks.length > read ? '; and ${_count(tasks.length - read).toLowerCase()} more' : '';
  return '$count: ${named.join('; ')}$more.';
}

/// "One task needs you", "Three tasks need you".
String needsYouCount(int n) => n == 1 ? 'One task needs you' : '${_count(n)} tasks need you';

/// What CarPlay says first about one waiting task: "Fix login, for Max. Waiting for you."
String taskIntro(TaskView task) {
  final who = task.employeeName == null ? '' : ', for ${task.employeeName}';
  return '${_plain(task.title)}$who. ${task.statusLabel}.';
}

/// What CarPlay says last about it: the question, or where to decide.
String taskPrompt(TaskView task) => answerableByVoice(task) ? 'What is your answer?' : 'This one needs your phone, in the Inbox.';

/// A dictated task or idea: the first sentence (at most [limit] characters,
/// cut at a word) is the title, the whole text the body.
({String title, String body}) dictatedNote(String said, {int limit = 80}) {
  final body = said.trim().replaceAll(RegExp(r'\s+'), ' ');
  var title = _plain(RegExp(r'^.+?[.!?](?=\s|$)').firstMatch(body)?[0] ?? body);
  if (title.length > limit) {
    final cut = title.lastIndexOf(' ', limit);
    title = '${title.substring(0, cut > limit ~/ 2 ? cut : limit).trimRight()}…';
  }
  if (title.isNotEmpty) title = title[0].toUpperCase() + title.substring(1);
  return (title: title, body: body);
}

String _plain(String title) => title.trim().replaceAll(RegExp(r'[.!?]+$'), '');

String _count(int n) => n >= 1 && n <= 10 ? const ['One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine', 'Ten'][n - 1] : '$n';
