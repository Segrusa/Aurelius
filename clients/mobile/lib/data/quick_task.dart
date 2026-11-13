import 'package:shared_preferences/shared_preferences.dart';

import '../core/net/connection.dart' show LinkState;
import 'protocol/protocol.dart';

// The quick task (ticket #53): a ticket kind for small ad-hoc work. It starts in
// Doing with the chosen employee, works on its own branch and is merged into the
// project's main when a board member approves it. Shown only on a server with
// [quickTaskFeature]; an older one would refuse `kind` (deny_unknown_fields).

/// The server has quick tasks (protocol 5, `FEATURES`): only then does a client show anything of them.
const quickTaskFeature = 'quick_task';

/// The wire value of `TicketView.kind` for a quick task (a normal ticket has none).
const quickKind = 'quick';

/// What the board and its lists call a quick task: the same words as the desktop.
const quickTaskName = 'Quick task';
const quickLabel = 'Quick';
const quickFilterName = 'Quick tasks';
const approveAndMerge = 'Approve and merge';

/// The question before a quick task's work is thrown away (BR4).
const quickCancelTitle = 'Cancel this quick task?';
const quickCancelBody = 'Its branch and worktree are removed; work that was not merged is lost.';

/// Whether [link]'s server has quick tasks.
bool quickTasksOn(LinkState? link) => link?.has(quickTaskFeature) ?? false;

/// Whether [ticket] is a quick task as this client shows it: never on a server without the feature.
bool isQuickTask(TicketView ticket, {required bool on}) => on && ticket.kind == quickKind;

/// The short form of a merge commit, as git shows it.
String shortCommit(String commit) => commit.length > 7 ? commit.substring(0, 7) : commit;

/// The `ticket.create` of a quick task: `kind` quick, the employee and the
/// description required, the title may be empty (the server takes the
/// description's first line). Never `start`: a quick task starts by itself.
Map<String, dynamic> quickTaskRequest({
  required String projectId,
  required String description,
  required String employeeId,
  String title = '',
  String? responsibleUserId,
  TicketPriority? priority,
  List<String> labels = const [],
  List<String> files = const [],
  int? deadlineMs,
  String? fromIdea,
  String? fromMessage,
}) =>
    ticketCreate(
      kind: TicketKind.quick,
      projectId: projectId,
      title: title.trim(),
      description: description.trim(),
      ownerEmployeeId: employeeId,
      responsibleUserId: responsibleUserId,
      priority: priority,
      labels: labels,
      files: files,
      deadlineMs: deadlineMs,
      fromIdea: fromIdea,
      fromMessage: fromMessage,
    );

/// The employee last given a quick task, kept on this phone per host so the
/// next one starts with them (ST-01 AC2). Loaded once per host.
class LastQuickEmployee {
  LastQuickEmployee._();

  static String? _host;
  static String? employee;

  static String _key(String hostId) => 'quick.last.$hostId';

  /// Read the saved choice of [hostId] (once per host).
  static Future<void> load(String hostId) async {
    if (_host == hostId) return;
    _host = hostId;
    employee = null;
    try {
      final saved = (await SharedPreferences.getInstance()).getString(_key(hostId));
      if (_host == hostId && saved != null && saved.isNotEmpty) employee = saved;
    } catch (_) {
      // No preferences (tests, a broken store): no default.
    }
  }

  /// Remember [employeeId] as the last one given a quick task on [hostId].
  static Future<void> remember(String hostId, String employeeId) async {
    _host = hostId;
    employee = employeeId;
    try {
      await (await SharedPreferences.getInstance()).setString(_key(hostId), employeeId);
    } catch (_) {
      // Not saved; it still holds until the app restarts.
    }
  }

  /// Forget what was loaded (tests).
  static void reset() {
    _host = null;
    employee = null;
  }
}
