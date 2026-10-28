import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../project/task_status_mark.dart';
import 'inbox_bloc.dart';

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
    this.variantTasks = const [],
    required this.runningTasks,
    required this.answers,
    required this.results,
  });

  factory InboxSections.of(InboxState inbox, CompanyState company) {
    // A task blocked on an approval shows as that approval's card, not twice.
    final asked = {for (final a in inbox.approvals) ?a.taskId};
    final open = company.tasks.where((t) => t.isOpen && t.status != TaskStatus.draft).toList()
      ..sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
    return InboxSections(
      approvals: inbox.approvals,
      proposals: inbox.proposals,
      // Variants ready to compare show as their compare card (feature `task_variants`).
      variantTasks: inbox.variantsOn ? open.where((t) => t.variantsReady && !asked.contains(t.id)).toList() : const [],
      waitingTasks: open.where((t) => taskMark(t) == TaskMark.needsYou && !asked.contains(t.id) && !(inbox.variantsOn && t.variantsReady)).toList(),
      // Everything else that has not ended: working, queued, in line or
      // waiting on a colleague.
      runningTasks: open.where((t) => taskMark(t) != TaskMark.needsYou).toList(),
      answers: [
        for (final m in company.workingEmployees)
          for (final a in m.agents)
            if (a.kind == 'conversation' && a.taskId == null) (employee: m, agent: a),
      ]..sort((a, b) => b.agent.sinceMs.compareTo(a.agent.sinceMs)),
      // "Action" notifications only announce what Needs you already shows live.
      results: inbox.notifications.where((n) => n.severity != 'action').toList(),
    );
  }

  final List<ApprovalView> approvals;
  final List<ProposalView> proposals;

  /// Tasks waiting for the user's input or decision without a pending approval.
  final List<TaskView> waitingTasks;

  /// Tasks whose variants wait for the user to choose one.
  final List<TaskView> variantTasks;
  final List<TaskView> runningTasks;
  final List<RunningAnswer> answers;

  /// Finished results; "Clear all" dismisses these (never approvals).
  final List<NotificationView> results;

  int get needsYou => approvals.length + proposals.length + variantTasks.length + waitingTasks.length;
  int get running => runningTasks.length + answers.length;
  int get done => results.length;
  bool get isEmpty => needsYou == 0 && running == 0 && done == 0;
}
