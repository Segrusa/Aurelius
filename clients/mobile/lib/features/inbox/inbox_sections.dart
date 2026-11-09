import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../project/task_status_mark.dart';
import 'inbox_bloc.dart';
import 'inbox_rows.dart';

/// An employee answering in one of its conversations, outside any task.
typedef RunningAnswer = ({EmployeeSummary employee, AgentView agent});

/// The inbox in three parts, in this order: what needs the user, what is
/// running, and finished results. Client-only: approvals, proposals and
/// results come from [InboxState], tasks and live answers from [CompanyState],
/// so items move between parts as either refreshes.
class InboxSections {
  const InboxSections({
    required this.approvals,
    required this.proposals,
    required this.waitingTasks,
    required this.runningTasks,
    required this.answers,
    required this.results,
    this.rowsNeedingYou = const [],
    this.updates = const [],
  });

  factory InboxSections.of(InboxState inbox, CompanyState company) {
    // A task blocked on an approval shows as that approval's card, not twice.
    final asked = {for (final a in inbox.approvals) ?a.taskId};
    final open = company.tasks.where((t) => t.isOpen && t.status != TaskStatus.draft).toList()
      ..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
    final rows = inbox.personalRows;
    return InboxSections(
      rowsNeedingYou: rows.where((n) => n.waitsForMe).toList(),
      updates: rows.where((n) => !n.waitsForMe).toList(),
      approvals: inbox.approvals,
      proposals: inbox.proposals,
      // A main task whose decision is someone else's is not yours to wait on (freeze §5).
      waitingTasks: open.where((t) => taskMark(t) == TaskMark.needsYou && !asked.contains(t.id) && !company.othersTasks.contains(t.id)).toList(),
      // Everything else that has not ended: working, queued, in line or
      // waiting on a colleague.
      runningTasks: open.where((t) => taskMark(t) != TaskMark.needsYou).toList(),
      answers: [
        for (final m in company.workingEmployees)
          for (final a in m.agents)
            if (a.kind == 'conversation' && a.taskId == null) (employee: m, agent: a),
      ]..sort((a, b) => b.agent.sinceMs.compareTo(a.agent.sinceMs)),
      // "Action" notifications only announce what Needs you already shows live.
      results: inbox.notifications.where((n) => n.severity != 'action' && !rows.contains(n)).toList(),
    );
  }

  final List<ApprovalView> approvals;
  final List<ProposalView> proposals;


  /// Tasks waiting for the user's input or decision without a pending approval.
  final List<TaskView> waitingTasks;

  final List<TaskView> runningTasks;
  final List<RunningAnswer> answers;

  /// Finished results; "Clear all" dismisses these (never approvals).
  final List<NotificationView> results;

  /// Rows of the personal Inbox with a ◆, under the approvals and proposals.
  final List<NotificationView> rowsNeedingYou;

  /// The other rows of the personal Inbox: Updates.
  final List<NotificationView> updates;

  int get needsYou => rowsNeedingYou.length + approvals.length + proposals.length + waitingTasks.length;
  int get running => runningTasks.length + answers.length;
  int get done => results.length;
  bool get isEmpty => needsYou == 0 && updates.isEmpty && running == 0 && done == 0;
}
