import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/quick_task.dart';
import '../../data/run_config.dart' show ModelCatalog;
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import '../markdown/links.dart';
import '../my_defaults/run_default_chips.dart';
import '../my_defaults/run_default_label.dart';
import 'ticket_actions.dart';
import 'ticket_bloc.dart';
import 'ticket_card.dart' show startWith;
import 'ticket_session_rules.dart';

// The gate of a ticket's session (ticket #71, ui-ontwerp §3.2–§3.4): right
// above the input, the decision of the moment for its column. Plan review and
// Review decide; every other column has the status box, which says nothing
// while the work simply runs. Each button is the write it was on the ticket
// page (same RPC, same confirmation); the ticket is read again right after.

/// The gate for [page]'s ticket. [composer] is the input under it: Request
/// changes and Send back take their words from it; without one (a server
/// without ticket conversations) they open the plan page or the feedback sheet.
class TicketGateView extends StatelessWidget {
  const TicketGateView({super.key, required this.page, required this.state, required this.online, this.composer, this.note});

  final TicketPageState page;
  final TicketState state;
  final bool online;
  final TextEditingController? composer;

  /// When the decision is someone else's: "Bo is responsible. You can decide too."
  final String? note;

  @override
  Widget build(BuildContext context) {
    final ticket = page.ticket!;
    return switch (ticketGate(ticket)) {
      TicketGate.none => const SizedBox.shrink(),
      TicketGate.plan => _PlanGate(ticket: ticket, plan: page.plan, online: online, composer: composer, note: note),
      TicketGate.review => _ReviewGate(ticket: ticket, root: page.root, online: online, composer: composer, note: note),
      TicketGate.status => _StatusBox(page: page, state: state, online: online),
    };
  }
}

/// Reads the ticket again at once after a write, not only on its event.
void _reread(BuildContext context) {
  try {
    final bloc = context.read<TicketBloc>();
    if (!bloc.isClosed) bloc.add(const TicketRefreshed());
  } on ProviderNotFoundException {
    // Outside a ticket's page: its event brings the change.
  }
}

/// The frame of a gate: a box with a head line, a body that scrolls when it is
/// long, and the buttons, which never hide.
class _GateBox extends StatelessWidget {
  const _GateBox({super.key, required this.title, this.attention = false, this.body = const [], this.buttons = const [], this.note});

  final String title;
  final bool attention;
  final List<Widget> body;
  final List<Widget> buttons;
  final String? note;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(12, 4, 12, 6),
    child: Container(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      decoration: BoxDecoration(
        color: Tokens.cell,
        borderRadius: BorderRadius.circular(18),
        border: attention ? Border.all(color: Tokens.accent.withValues(alpha: 0.45)) : Border.all(color: Tokens.sep, width: 0.5),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            title,
            style: TextStyle(fontWeight: FontWeight.w600, color: attention ? Tokens.accentText : Tokens.label),
          ),
          // While typing only the head and the buttons: the input and the timeline keep their room.
          if (body.isNotEmpty && MediaQuery.viewInsetsOf(context).bottom == 0)
            ConstrainedBox(
              // The gate takes at most a part of the screen; its body scrolls inside it.
              constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.12),
              child: SingleChildScrollView(
                padding: const EdgeInsets.only(top: 4),
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: body),
              ),
            ),
          if (note != null)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: Text(
                note!,
                key: const ValueKey('bar-others-note'),
                style: TextStyle(color: Tokens.label2, fontSize: 13),
              ),
            ),
          if (buttons.isNotEmpty) ...[
            const SizedBox(height: 8),
            // All of them in sight on a phone: they wrap rather than scroll.
            Theme(
              data: Theme.of(context).copyWith(
                filledButtonTheme: FilledButtonThemeData(style: _compact.merge(Theme.of(context).filledButtonTheme.style)),
                outlinedButtonTheme: OutlinedButtonThemeData(style: _compact.merge(Theme.of(context).outlinedButtonTheme.style)),
                textButtonTheme: TextButtonThemeData(style: _compact.merge(Theme.of(context).textButtonTheme.style)),
              ),
              child: Wrap(spacing: 6, runSpacing: 6, crossAxisAlignment: WrapCrossAlignment.center, children: buttons),
            ),
          ],
        ],
      ),
    ),
  );
}

Widget _secondary(String text, {Key? key}) => Text(
  text,
  key: key,
  style: TextStyle(color: Tokens.label2),
);

/// "Show the whole plan": a link, as small as its text.
final _link = TextButton.styleFrom(padding: EdgeInsets.zero, minimumSize: Size.zero, tapTargetSize: MaterialTapTargetSize.shrinkWrap, visualDensity: VisualDensity.compact);

/// The gate's buttons are small: the timeline above them keeps its room on a phone.
final _compact = ButtonStyle(visualDensity: VisualDensity.compact, tapTargetSize: MaterialTapTargetSize.shrinkWrap, minimumSize: const WidgetStatePropertyAll(Size(0, 36)), padding: const WidgetStatePropertyAll(EdgeInsets.symmetric(horizontal: 12)));

/// Plan review (ui-ontwerp §3.3): the plan, Approve, Approve, accept edits,
/// Request changes and Reject. Words in the input go along with Approve as a
/// note and are what Request changes asks for (B2: it needs them).
class _PlanGate extends StatefulWidget {
  const _PlanGate({required this.ticket, required this.plan, required this.online, this.composer, this.note});

  final TicketView ticket;
  final String? plan;
  final bool online;
  final TextEditingController? composer;
  final String? note;

  @override
  State<_PlanGate> createState() => _PlanGateState();
}

class _PlanGateState extends State<_PlanGate> {
  bool _busy = false;

  Future<void> _decide(Map<String, dynamic> request, String done) async {
    HapticFeedback.mediumImpact();
    setState(() => _busy = true);
    final ok = await ticketWrite(context, request, expectedVersion: widget.ticket.version, retry: false, done: done);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) {
      widget.composer?.clear();
      _reread(context);
    }
  }

  @override
  Widget build(BuildContext context) {
    final ticket = widget.ticket;
    final composer = widget.composer;
    final plan = (widget.plan ?? '').trim();
    return ListenableBuilder(
      listenable: composer ?? ValueNotifier(''),
      builder: (context, _) {
        final can = widget.online && !_busy;
        final words = composer?.text.trim() ?? '';
        final note = words.isEmpty ? null : words;
        return _GateBox(
          key: const ValueKey('gate-plan'),
          attention: true,
          title: 'Plan review · your decision',
          note: widget.note,
          body: [
            if (plan.isEmpty) _secondary('This agent did not write a plan of its own.') else MessageMarkdown(key: const ValueKey('gate-plan-text'), data: plan, scope: null, selectable: true),
            TextButton(key: const ValueKey('gate-whole-plan'), style: _link, onPressed: () => context.push('/ticket/${ticket.id}/plan'), child: const Text('Show the whole plan')),
          ],
          buttons: [
            FilledButton(
              key: const ValueKey('gate-approve'),
              onPressed: can ? () => _decide(ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approve, feedback: note), 'The plan is approved. The work goes on in Auto.') : null,
              child: const Text('Approve'),
            ),
            OutlinedButton(
              key: const ValueKey('gate-approve-edits'),
              onPressed: can ? () => _decide(ticketApprovePlan(ticketId: ticket.id, decision: PlanApproval.approveEdits, feedback: note), 'The plan is approved. Edits go through; the rest is asked.') : null,
              child: const Text('Approve, accept edits'),
            ),
            OutlinedButton(
              key: const ValueKey('gate-revise'),
              onPressed: !can
                  ? null
                  : composer == null
                  ? () => context.push('/ticket/${ticket.id}/plan')
                  : note == null
                  ? () => toast(context, 'Write what needs to change below, then Request changes.')
                  : () => _decide(ticketRevisePlan(ticketId: ticket.id, feedback: note), 'Changes requested. ${ticket.ownerName ?? 'The owner'} makes a new plan.'),
              child: const Text('Request changes'),
            ),
            TextButton(
              key: const ValueKey('gate-reject'),
              style: TextButton.styleFrom(foregroundColor: Tokens.negative),
              onPressed: can
                  ? () async {
                      if (await backToBacklog(context, ticket) && context.mounted) _reread(context);
                    }
                  : null,
              child: const Text('Reject'),
            ),
          ],
        );
      },
    );
  }
}

/// Review (ui-ontwerp §3.2): the report, Done (Approve and merge for a quick
/// task) and Send back to Doing, whose feedback is the input's text.
class _ReviewGate extends StatefulWidget {
  const _ReviewGate({required this.ticket, required this.root, required this.online, this.composer, this.note});

  final TicketView ticket;
  final TaskView? root;
  final bool online;
  final TextEditingController? composer;
  final String? note;

  @override
  State<_ReviewGate> createState() => _ReviewGateState();
}

class _ReviewGateState extends State<_ReviewGate> {
  bool _busy = false;

  Future<void> _sendBack(String feedback) async {
    final ticket = widget.ticket;
    HapticFeedback.mediumImpact();
    setState(() => _busy = true);
    final ok = await ticketWrite(
      context,
      ticketSendBack(ticketId: ticket.id, feedback: feedback),
      expectedVersion: ticket.version,
      retry: false,
      done: '#${ticket.number} is back in Doing with your feedback.',
    );
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) {
      widget.composer?.clear();
      _reread(context);
    }
  }

  @override
  Widget build(BuildContext context) {
    final ticket = widget.ticket;
    final root = widget.root;
    final composer = widget.composer;
    final report = (root?.result ?? '').trim();
    final owner = ticket.ownerName ?? 'the owner';
    return ListenableBuilder(
      listenable: composer ?? ValueNotifier(''),
      builder: (context, _) {
        final can = widget.online && !_busy;
        final words = composer?.text.trim() ?? '';
        return _GateBox(
          key: const ValueKey('gate-review'),
          attention: true,
          title: 'In review · your decision',
          note: widget.note,
          body: [
            Text('From $owner${root == null ? '' : ' · ${Fmt.ago(root.updatedAtMs)}'}', style: TextStyle(color: Tokens.label2, fontSize: 13)),
            const SizedBox(height: 4),
            if (report.isEmpty) _secondary('No report text. See the conversation.') else MessageMarkdown(key: const ValueKey('ticket-report'), data: report, scope: null, selectable: true),
            if (report.isNotEmpty) TextButton(key: const ValueKey('gate-whole-report'), style: _link, onPressed: () => context.push('/ticket/${ticket.id}/report'), child: const Text('Show the whole report')),
          ],
          buttons: [
            FilledButton.icon(
              key: const ValueKey('bar-done'),
              onPressed: can
                  ? () async {
                      if (await showMarkDone(context, ticket) && context.mounted) _reread(context);
                    }
                  : null,
              icon: const Icon(Icons.check, size: 18),
              label: Text(acceptLabel(ticket, quickOn: quickTasksOn(context.read<ConnectionBloc>().state))),
            ),
            OutlinedButton(
              key: const ValueKey('bar-send-back'),
              onPressed: !can
                  ? null
                  : composer == null
                  ? () async {
                      if (await showSendBack(context, ticket) && context.mounted) _reread(context);
                    }
                  : words.isEmpty
                  ? () => toast(context, 'Write what needs to change below, then Send back.')
                  : () => _sendBack(words),
              child: const Text('Send back to Doing'),
            ),
          ],
        );
      },
    );
  }
}

/// Every other open column (ui-ontwerp §3.4, the desktop's StatusBox): the
/// login wait, failed, Backlog, Todo, a main task that stopped, Testing.
/// Nothing while the work simply runs: the timeline says what runs.
class _StatusBox extends StatelessWidget {
  const _StatusBox({required this.page, required this.state, required this.online});

  final TicketPageState page;
  final TicketState state;
  final bool online;

  @override
  Widget build(BuildContext context) {
    final ticket = page.ticket!;
    final root = page.root;
    final company = context.watch<CompanyBloc>().state;
    final link = context.watch<ConnectionBloc>().state;
    final me = link.user?.id;
    final mine = needsMe(ticket, state, me: me);
    final owner = ticket.ownerName ?? employeeName(company, ticket.ownerEmployeeId);
    final dispatcher = dispatcherName(company, ticket.projectId);
    final expired = link.claudeLogin?.status == ClaudeLoginStatus.expired;
    VoidCallback? on(Future<Object?> Function() f) => online
        ? () async {
            await f();
            if (context.mounted) _reread(context);
          }
        : null;
    final backlog = OutlinedButton(key: const ValueKey('bar-backlog'), onPressed: on(() => backToBacklog(context, ticket)), child: const Text('Back to Backlog'));
    final choose = FilledButton(key: const ValueKey('bar-choose'), onPressed: on(() => chooseOwner(context, ticket)), child: const Text('Choose employee'));
    // A main task that stopped for input waits for you too; the desktop shows it as stopped, with Try again.
    if ((state == TicketState.working || state == TicketState.waitingForYou) && mainTaskStopped(ticket, root)) return _stoppedBox(context, root!, owner, on, backlog);
    switch (state) {
      case TicketState.needsYourLogin:
        return _GateBox(
          key: const ValueKey('ticket-needs-login'),
          attention: true,
          title: '◆ ${state.line(expired: expired)}',
          body: [_secondary('It runs only on your own Claude subscription. It goes on by itself once your login is connected.')],
          buttons: [
            FilledButton(key: const ValueKey('bar-connect'), onPressed: () => context.push('/account/claude'), child: Text(expired ? 'Reconnect Claude login' : 'Connect Claude login')),
            backlog,
          ],
        );
      case TicketState.waitsForLogin:
        return _GateBox(
          key: const ValueKey('ticket-waits-login'),
          title: '⏸ ${state.line(login: ticket.loginUsername)}',
          body: [_secondary("It runs only on ${ticket.loginUsername}'s subscription and goes on once that login is connected. Or run it on your own login.")],
          buttons: [
            FilledButton(key: const ValueKey('bar-use-my-login'), onPressed: on(() => runOnMyLogin(context, ticket)), child: const Text('Run on my login')),
            backlog,
          ],
        );
      case TicketState.waitingForLimit:
        return _GateBox(
          key: const ValueKey('ticket-limit'),
          title: '⏸ ${state.line(login: ticket.loginUsername, mine: ticket.loginUser == me, untilMs: root?.waitingUntilMs)}',
          body: [_secondary('Runs only on the subscription of the login it runs on. It goes on by itself after the reset.')],
        );
      case TicketState.failed:
        return _GateBox(
          key: const ValueKey('ticket-failed'),
          attention: mine,
          title: '✕ The main task failed',
          body: [_secondary(root?.result ?? 'Start again for a new main task; the old one stays linked.')],
          buttons: [
            FilledButton(key: const ValueKey('bar-start-again'), onPressed: on(() => startTicket(context, ticket)), child: const Text('Start again')),
            backlog,
          ],
        );
      case TicketState.backlog:
        return _GateBox(
          key: const ValueKey('ticket-backlog'),
          title: 'In Backlog · not started',
          buttons: [
            FilledButton(key: const ValueKey('bar-start'), onPressed: on(() => startTicket(context, ticket)), child: const Text('▸ Start')),
            OutlinedButton(key: const ValueKey('bar-start-with'), onPressed: on(() => startWith(context, ticket)), child: const Text('Start with…')),
            TextButton(
              key: const ValueKey('bar-delete'),
              style: TextButton.styleFrom(foregroundColor: Tokens.negative),
              onPressed: on(() => deleteTicket(context, ticket)),
              child: const Text('Delete'),
            ),
          ],
        );
      case TicketState.choosing:
        return _GateBox(title: '◌ ${dispatcher ?? 'The dispatcher'} is choosing an employee…', body: [_secondary('You can choose one yourself.')], buttons: [choose, backlog]);
      case TicketState.needsOwner:
        final inactive = dispatcher == null ? inactiveDispatcher(company, ticket.projectId) : null;
        return _GateBox(key: const ValueKey('ticket-needs-owner'), attention: mine, title: mine ? '◆ Needs you · choose an owner' : 'Choose an owner', body: [_secondary(dispatcher != null ? '$dispatcher could not choose an employee.' : '${inactive ?? 'No dispatcher is set for this project'}. Started tickets wait in Todo until you choose an owner.')], buttons: [choose, backlog]);
      case TicketState.ownerUnknown:
        return _GateBox(
          key: const ValueKey('ticket-owner-unknown'),
          attention: mine,
          title: mine ? '◆ Needs you · choose an owner' : 'Choose an owner',
          body: [_secondary('Could not check whether ${dispatcher ?? 'the dispatcher'} is still choosing. Choose an employee yourself, or try again.')],
          buttons: [
            choose,
            OutlinedButton(onPressed: () => _reread(context), child: const Text('Try again')),
            backlog,
          ],
        );
      case TicketState.queued:
        return _GateBox(title: 'Queued for ${owner ?? 'its owner'}.', buttons: [backlog]);
      case TicketState.waitingForYou:
        return _GateBox(
          key: const ValueKey('ticket-waiting-for-you'),
          attention: mine,
          title: mine ? '◆ Waiting for your approval' : 'Waiting for an approval',
          body: [_secondary('Answer it in the Inbox.')],
          buttons: [FilledButton(onPressed: () => context.go('/inbox'), child: const Text('Open Inbox'))],
        );
      case TicketState.working:
        return const SizedBox.shrink();
      case TicketState.testing:
        final test = testTaskOf(page.detail!.tasks, testerId: company.company?.testerEmployeeId);
        return _GateBox(
          key: const ValueKey('ticket-testing'),
          title: '${test?.employeeName ?? testerName(company) ?? 'The tester'} is testing',
          body: [_secondary('The full test suite runs once. Green goes on to Review; red goes back to Doing with the failures for the owner.')],
          buttons: [if (test != null) OutlinedButton(key: const ValueKey('bar-open-test-task'), onPressed: () => context.push('/project/${test.projectId}/task/${test.id}'), child: const Text('Open test task'))],
        );
      case TicketState.planReview || TicketState.review || TicketState.done:
        return const SizedBox.shrink();
    }
  }

  /// Doing, and the main task stopped and waits for input: its failure, Try
  /// again (its last question once more, as the desktop) and Back to Backlog.
  Widget _stoppedBox(BuildContext context, TaskView root, String? owner, VoidCallback? Function(Future<Object?> Function()) on, Widget backlog) {
    final stopped = page.stopped;
    return _GateBox(
      key: const ValueKey('ticket-stopped'),
      attention: true,
      title: '${owner ?? 'The employee'} stopped and waits for you.',
      body: [if (stopped?.failure case final failure?) _secondary(failure, key: const ValueKey('ticket-stopped-failure'))],
      buttons: [
        if (stopped?.failure != null && root.employeeId != null && root.conversationId != null)
          FilledButton(
            key: const ValueKey('bar-try-again'),
            onPressed: on(
              () => ticketWrite(
                context,
                employeeSendMessage(SendEmployeeMessage(conversationId: root.conversationId!, employeeId: root.employeeId!, text: stopped!.retry)),
                retry: false,
                done: 'Asked ${owner ?? 'the employee'} again.',
              ),
            ),
            child: const Text('Try again'),
          ),
        if (root.conversationId case final c?) OutlinedButton(key: const ValueKey('bar-open-task'), onPressed: () => context.push('/conversation/$c?e=${root.employeeId ?? ''}'), child: const Text('Open task')),
        backlog,
      ],
    );
  }
}

/// Under the input (ui-ontwerp §5.3): the run chips Mode and Model
/// (`ticket.update { run }`, as on the ticket page) and what a name does (BR1).
class TicketUnderInput extends StatelessWidget {
  const TicketUnderInput({super.key, required this.ticket, required this.online, this.owner});

  final TicketView ticket;
  final bool online;
  final String? owner;

  @override
  Widget build(BuildContext context) {
    final chips = runChips(ticket, quick: quickTaskOf(context, ticket));
    // The ticket runs on its login user's defaults: "Your default · …" or "Kim's default · …".
    final labels = watchRunDefaultLabels(context, loginId: ticket.loginUser, loginName: ticket.loginUsername);
    return Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (chips.mode || chips.model)
            Wrap(
              spacing: 6,
              runSpacing: 4,
              children: [
                if (chips.mode)
                  ActionChip(
                    key: const ValueKey('chip-mode'),
                    visualDensity: VisualDensity.compact,
                    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    avatar: Icon(Icons.tune, size: 16, color: Tokens.text2),
                    label: Text('Mode · ${modeChipLabel(ticket.run, labels?.mode ?? 'Default')}'),
                    onPressed: online ? () => chooseTicketMode(context, ticket) : null,
                  ),
                if (chips.model)
                  ActionChip(
                    key: const ValueKey('chip-model'),
                    visualDensity: VisualDensity.compact,
                    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    avatar: Icon(Icons.memory, size: 16, color: Tokens.text2),
                    label: Text('Model · ${modelChipLabel(ticket.run, labels?.model ?? 'Default')}'),
                    onPressed: online ? () => chooseTicketModel(context, ticket) : null,
                  ),
                if (chips.model && labels != null)
                  ActionChip(
                    key: const ValueKey('chip-effort'),
                    visualDensity: VisualDensity.compact,
                    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    avatar: Icon(Icons.speed, size: 16, color: Tokens.text2),
                    label: Text('Effort · ${effortChipLabel(ticket.run, labels.effort)}'),
                    onPressed: online ? () => chooseTicketEffort(context, ticket) : null,
                  ),
              ],
            ),
          Padding(
            padding: const EdgeInsets.fromLTRB(4, 4, 4, 0),
            child: Text(
              sessionHint(ticket, owner: owner),
              key: const ValueKey('ticket-hint'),
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(color: Tokens.label2, fontSize: 12),
            ),
          ),
        ],
      ),
    );
  }
}

/// The labels of [ticket]'s defaults: its login user's.
RunDefaultLabels? _labelsOf(BuildContext context, TicketView ticket) =>
    readRunDefaultLabels(context, loginId: ticket.loginUser, loginName: ticket.loginUsername);

/// The Effort of [ticket]'s runs (`ticket.update { run }`, a server with
/// `user_run_defaults`): the default or one of the levels.
Future<bool> chooseTicketEffort(BuildContext context, TicketView ticket) async {
  final labels = _labelsOf(context, ticket);
  if (labels == null) return false;
  final picked = await showModalBottomSheet<(String?,)>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            key: const ValueKey('effort-default'),
            title: Text(labels.reset),
            subtitle: Text(labels.effortValue ?? ''),
            trailing: ticket.run.effort == null ? Icon(Icons.check, color: Tokens.accentText) : null,
            onTap: () => Navigator.pop(sheet, (null,)),
          ),
          for (final (value, label, detail) in ModelCatalog.efforts)
            ListTile(
              key: ValueKey('effort-$value'),
              title: Text(label),
              subtitle: Text(detail),
              trailing: ticket.run.effort == value ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (value,)),
            ),
          ChangeMyDefaultsTile(sheet: sheet),
        ],
      ),
    ),
  );
  if (picked == null || picked.$1 == ticket.run.effort || !context.mounted) return false;
  final ok = await ticketWrite(
    context,
    ticketUpdate(ticketId: ticket.id, run: TaskRun(mode: ticket.run.mode, model: ticket.run.model, effort: picked.$1)),
    expectedVersion: ticket.version,
  );
  if (ok && context.mounted) _reread(context);
  return ok;
}

/// What the first row of a ticket's run sheet says: "Use your default" with
/// the value, or on an older server "Default" with whose it was.
(String, String) _defaultRow(RunDefaultLabels? labels, String value, String old) =>
    labels == null ? ('Default', old) : (labels.reset, value);

/// The Mode of [ticket]'s runs (`ticket.update { run }`): the default (the
/// ticket's login user's, on an older server the owner's) or one of the modes.
Future<bool> chooseTicketMode(BuildContext context, TicketView ticket) async {
  final labels = _labelsOf(context, ticket);
  final (title, detail) = _defaultRow(labels, labels?.modeValue ?? '', "The owner's own mode");
  final picked = await showModalBottomSheet<(AgentMode?,)>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    builder: (sheet) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          ListTile(
            key: const ValueKey('mode-default'),
            title: Text(title),
            subtitle: Text(detail),
            trailing: ticket.run.mode == null ? Icon(Icons.check, color: Tokens.accentText) : null,
            onTap: () => Navigator.pop(sheet, (null,)),
          ),
          for (final m in agentModes)
            ListTile(
              key: ValueKey('mode-${m.wire}'),
              title: Text(m.label),
              subtitle: m == AgentMode.plan ? const Text('Stops in Plan review with a plan before anything changes.') : null,
              trailing: ticket.run.mode == m ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (m,)),
            ),
          if (labels != null) ChangeMyDefaultsTile(sheet: sheet),
        ],
      ),
    ),
  );
  if (picked == null || picked.$1 == ticket.run.mode || !context.mounted) return false;
  final ok = await ticketWrite(
    context,
    ticketUpdate(
      ticketId: ticket.id,
      run: TaskRun(mode: picked.$1, model: ticket.run.model, effort: ticket.run.effort),
    ),
    expectedVersion: ticket.version,
  );
  if (ok && context.mounted) _reread(context);
  return ok;
}

/// The Model of [ticket]'s runs (`ticket.update { run }`): Default or one of
/// the models the connections report; a failed read says so.
Future<bool> chooseTicketModel(BuildContext context, TicketView ticket) async {
  final List<String> models;
  try {
    models = modelChoices([for (final c in (await context.read<AureliusRepository>().connections()).connections) c.models]);
  } on Object catch (e) {
    if (context.mounted) toast(context, ticketFailure(e));
    return false;
  }
  if (!context.mounted) return false;
  final labels = _labelsOf(context, ticket);
  final (title, detail) = _defaultRow(labels, labels?.modelValue ?? '', "The owner's own model");
  final picked = await showModalBottomSheet<(String?,)>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(
              key: const ValueKey('model-default'),
              title: Text(title),
              subtitle: Text(detail),
              trailing: ticket.run.model == null ? Icon(Icons.check, color: Tokens.accentText) : null,
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
            if (models.isEmpty)
              ListTile(
                title: Text('The connections report no models.', style: TextStyle(color: Tokens.label2)),
              ),
            for (final m in models)
              ListTile(
                key: ValueKey('model-$m'),
                title: Text(ModelCatalog.friendly(m)),
                subtitle: ModelCatalog.describe(m) == null ? null : Text(ModelCatalog.describe(m)!),
                trailing: ticket.run.model == m ? Icon(Icons.check, color: Tokens.accentText) : null,
                onTap: () => Navigator.pop(sheet, (m,)),
              ),
            if (labels != null) ChangeMyDefaultsTile(sheet: sheet),
          ],
        ),
      ),
    ),
  );
  if (picked == null || picked.$1 == ticket.run.model || !context.mounted) return false;
  final ok = await ticketWrite(
    context,
    ticketUpdate(
      ticketId: ticket.id,
      run: TaskRun(mode: ticket.run.mode, model: picked.$1, effort: ticket.run.effort),
    ),
    expectedVersion: ticket.version,
  );
  if (ok && context.mounted) _reread(context);
  return ok;
}
