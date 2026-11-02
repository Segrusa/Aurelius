import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:go_router/go_router.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../core/theme/tokens.dart';
import '../../core/widgets/dialogs.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../../data/company.dart';
import '../../data/discussion.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import '../account/claude_login.dart' show claudeLoginMissing;
import '../chat/secret_check.dart' show sendAnywayDialog;
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';

// The writes a board member makes on a ticket (ui-ontwerp §6, §8, §9): every
// button, swipe, "Move to…" and drop on a tablet ends here, so they all follow
// the same rules and say the same things. The server checks them again; its
// events bring the new board.

/// What the user sees when a ticket write fails.
String ticketFailure(Object e) => switch (e) {
      ProtocolException(isConflict: true) => 'Changed on another device. Showing the latest.',
      ProtocolException() => e.shown,
      NotSentException() => e.toString(),
      DisconnectedException() => e.message,
      _ => '$e',
    };

/// Send one ticket write; [done] is shown after it committed, the failure
/// otherwise. A toast with an
/// [action] (Undo) still goes after 5 s (§6): an action alone would keep it
/// (`persist`). Decisions ([retry] false) are never replayed later.
Future<bool> ticketWrite(BuildContext context, Map<String, dynamic> request, {int? expectedVersion, String? done, bool retry = true, SnackBarAction? action}) async {
  final repository = context.read<AureliusRepository>();
  final messenger = ScaffoldMessenger.maybeOf(context);
  void show(String text, [SnackBarAction? action]) => messenger
    ?..hideCurrentSnackBar()
    ..showSnackBar(SnackBar(content: Text(text), action: action, persist: false, duration: action == null ? const Duration(seconds: 4) : const Duration(seconds: 5)));
  try {
    // B10 (AUR-MU-04-F5): a text holding a vault value asks first; Send anyway
    // writes it again with `confirm_secret`, once.
    final sent = await writeConfirmingSecret(repository, request, sendAnywayDialog(context), expectedVersion: expectedVersion, retry: retry);
    if (sent == null) return false;
    if (done != null) show(done, action);
    return true;
  } on ProtocolException catch (e) {
    show(ticketFailure(e));
    return false;
  } on Object catch (e) {
    show(ticketFailure(e));
    return false;
  }
}

/// The name of the employee [id], or null.
String? employeeName(CompanyState company, String? id) => id == null ? null : company.employee(id)?.name;

/// Who chooses owners for [ticket]'s project, by name; null when nobody does.
String? dispatcherName(CompanyState company, String projectId) =>
    employeeName(company, effectiveDispatcher(company.project(projectId), company.company?.dispatcherEmployeeId, company.employees));

/// Who runs the tests of a card in Testing, by name (the company's tester,
/// ticket #5); null when there is none or it is unknown.
String? testerName(CompanyState company) => employeeName(company, company.company?.testerEmployeeId);

/// Why nobody chooses owners for [projectId]'s tickets when a dispatcher is
/// chosen but not active ("Vesta is archived"); null otherwise. Nobody takes
/// over (B5 strict).
String? inactiveDispatcher(CompanyState company, String projectId) {
  final id = chosenDispatcher(company.project(projectId), company.company?.dispatcherEmployeeId);
  if (id == null) return null;
  final employee = company.employee(id);
  return switch (employee?.status) {
    LifecycleStatus.active => null,
    LifecycleStatus.paused => '${employee!.name} is paused',
    LifecycleStatus.archived => '${employee!.name} is archived',
    null => 'Its dispatcher is no longer active',
  };
}

/// Start (Backlog → Todo, MU1-09): with [employeeId] the dispatcher is skipped
/// (MU1-12). The toast says what happens next and offers Undo while no owner
/// is chosen (§6). Also "Start again" after a failed main task (B6).
Future<bool> startTicket(BuildContext context, TicketView ticket, {String? employeeId}) async {
  final company = context.read<CompanyBloc>().state;
  final owner = employeeName(company, employeeId ?? ticket.ownerEmployeeId);
  final dispatcher = dispatcherName(company, ticket.projectId);
  final again = ticket.column == ColumnKind.doing || ticket.rootTaskStatus == TaskStatus.failed;
  final next = owner != null
      ? 'started with $owner.'
      : dispatcher != null
          ? 'started. $dispatcher chooses an employee.'
          : 'started. Choose an owner: ${inactiveDispatcher(company, ticket.projectId) ?? 'no dispatcher is set'}.';
  // P-36: Start stays possible without a Claude login; the toast says it waits.
  final waitsForLogin = waitsForMyLogin(context.read<ConnectionBloc?>()?.state, ticket.loginUser);
  HapticFeedback.mediumImpact();
  final undo = owner == null && !again ? SnackBarAction(label: 'Undo', onPressed: () => _undoStart(context, ticket)) : null;
  return ticketWrite(
    context,
    ticketStart(ticketId: ticket.id, employeeId: employeeId),
    expectedVersion: ticket.version,
    done: waitsForLogin ? '#${ticket.number} started. $waitsForLoginNote' : again ? '#${ticket.number} starts again.' : '#${ticket.number} $next',
    action: undo,
  );
}

/// P-36: a ticket on [loginUser]'s login waits when that is the own login
/// and it is not connected (or not read yet, R3 m5).
bool waitsForMyLogin(LinkState? link, String? loginUser) => link != null && loginUser == link.user?.id && claudeLoginMissing(link);

/// What P-36 adds to the toast of a start that waits for the own login.
const waitsForLoginNote = 'It waits until you connect your Claude login.';

/// "Run on my login" (B4, D3-3, OV-06): a ticket waiting for another board
/// member's Claude login goes on the own login, after a confirmation. Never
/// someone else's: the server sets the one who asks.
Future<bool> runOnMyLogin(BuildContext context, TicketView ticket) async {
  final ok = await confirm(
    context,
    title: 'Run #${ticket.number} on your Claude login?',
    body: "Its runs, delegations included, then use your own Claude subscription instead of ${ticket.loginUsername}'s. ${ticket.createdByUsername} stays its creator.",
    action: 'Run on my login',
  );
  if (!ok || !context.mounted) return false;
  return ticketWrite(context, ticketUseMyLogin(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} runs on your Claude login now.');
}

/// Undo of a Start: only while no owner is chosen (§6). The ticket is read
/// first; once the dispatcher chose, Undo cancels nothing and says so. The
/// cancel carries that version, so a choice in between is not run over.
Future<void> _undoStart(BuildContext context, TicketView started) async {
  if (!context.mounted) return;
  final repository = context.read<AureliusRepository>();
  final company = context.read<CompanyBloc>().state;
  final TicketView now;
  try {
    now = (await repository.ticket(started.id)).ticket;
  } on Object catch (e) {
    if (context.mounted) toast(context, ticketFailure(e));
    return;
  }
  if (!context.mounted) return;
  if (now.column != ColumnKind.todo || now.ownerEmployeeId != null) {
    final who = now.ownerName ?? employeeName(company, now.ownerEmployeeId);
    toast(context, '#${started.number} ${who == null ? 'is already under way' : 'already went to $who'}; use Back to Backlog to stop it.');
    return;
  }
  await ticketWrite(context, ticketCancel(ticketId: started.id), expectedVersion: now.version, retry: false, done: '#${started.number} is back in Backlog.');
}

/// Back to Backlog: stops the work (`ticket.cancel`), after a confirmation
/// where work runs; in Plan review it is "Afwijzen".
Future<bool> backToBacklog(BuildContext context, TicketView ticket) async {
  final rule = moveRule(ticket.column, ColumnKind.backlog);
  if (rule is! MoveAllowed) return false;
  if (rule.confirm != null) {
    // In Plan review this is rejecting the plan; it says so, as the plan page's button does.
    final reject = ticket.column == ColumnKind.planReview;
    final ok = await confirm(context, title: reject ? 'Afwijzen?' : 'Back to Backlog?', body: rule.confirm, action: reject ? 'Afwijzen' : 'Back to Backlog', danger: true);
    if (!ok || !context.mounted) return false;
  }
  return ticketWrite(context, ticketCancel(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is back in Backlog.');
}

/// A card moved to [target] (Move to…, a drop on a tablet, a reorder): what
/// the rules of §6 make of it. [before] is the card it lands above (null: at the bottom).
Future<bool> moveTicket(BuildContext context, TicketView ticket, BoardColumnView target, {String? before}) async {
  final rule = moveRule(ticket.column, target.kind);
  switch (rule) {
    case MoveRefused(:final reason):
      toast(context, reason);
      return false;
    case MoveAllowed(kind: MoveKind.reorder):
      return ticketWrite(context, ticketMove(ticketId: ticket.id, columnId: target.id, beforeTicketId: before), expectedVersion: ticket.version);
    case MoveAllowed(kind: MoveKind.start):
      return startTicket(context, ticket);
    case MoveAllowed(kind: MoveKind.cancel):
      return backToBacklog(context, ticket);
    case MoveAllowed(kind: MoveKind.reviewPlan):
      context.push('/ticket/${ticket.id}/plan');
      return false;
    case MoveAllowed(kind: MoveKind.sendBack):
      // Phase 2: the feedback is a message of the discussion (MU2-06 AC1, MU2-05 AC4).
      if (_discussed(context, ticket)) {
        toast(context, 'Write what needs to change, then Send back.');
        context.push('/ticket/${ticket.id}?tab=discussion');
        return false;
      }
      return showSendBack(context, ticket);
    case MoveAllowed(kind: MoveKind.accept):
      return showMarkDone(context, ticket);
  }
}

/// The server keeps a discussion for [ticket] (feature `ticket_discussion`).
bool _discussed(BuildContext context, TicketView ticket) {
  try {
    return ticket.conversationId != null && context.read<ConnectionBloc>().state.has(ticketDiscussionFeature);
  } on ProviderNotFoundException {
    return false;
  }
}

/// Up or down one place within its column (the phone's reorder without dragging).
Future<bool> nudgeTicket(BuildContext context, TicketView ticket, BoardColumnCards column, int by) async {
  final index = column.tickets.indexWhere((t) => t.id == ticket.id);
  final to = index + by;
  if (index < 0 || to < 0 || to >= column.tickets.length) return false;
  return moveTicket(context, ticket, column.column, before: beforeTicketAt(column.tickets, ticket.id, to));
}

/// The sheet "Move #42 to…" (§4.2): every column in board order; the ones the
/// rules forbid stay in the list, dimmed, with the reason.
Future<void> showMoveSheet(BuildContext context, TicketView ticket, List<BoardColumnView> columns) async {
  final target = await showModalBottomSheet<BoardColumnView>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
            child: Text('Move #${ticket.number} ${ticket.title}', maxLines: 2, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600)),
          ),
          for (final c in columns)
            Builder(builder: (_) {
              final current = c.id == ticket.columnId;
              final rule = current ? null : moveRule(ticket.column, c.kind);
              final allowed = rule is MoveAllowed;
              return ListTile(
                key: ValueKey('move-to-${c.kind.wire}'),
                enabled: allowed,
                title: Text(c.name),
                subtitle: Text(
                  switch (rule) {
                    null => 'Current',
                    MoveRefused(:final reason) => reason,
                    MoveAllowed(kind: MoveKind.start) => 'Starts the ticket',
                    MoveAllowed(kind: MoveKind.cancel) => 'Stops the work',
                    MoveAllowed(kind: MoveKind.reviewPlan) => 'Opens the plan',
                    MoveAllowed(kind: MoveKind.sendBack) => 'With feedback for the owner',
                    MoveAllowed(kind: MoveKind.accept) => 'Closes the ticket',
                    MoveAllowed() => '',
                  },
                  style: TextStyle(color: Tokens.label2),
                ),
                trailing: current ? Icon(Icons.check, color: Tokens.accentText, semanticLabel: 'Current') : (allowed ? const Icon(CupertinoIcons.chevron_forward, size: 17) : null),
                onTap: allowed ? () => Navigator.pop(sheet, c) : null,
              );
            }),
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 12),
            child: OutlinedButton(onPressed: () => Navigator.pop(sheet), child: const Text('Cancel')),
          ),
        ],
      ),
    ),
  );
  if (target != null && context.mounted) await moveTicket(context, ticket, target);
}

/// The owner (MU1-12): in Backlog anyone or the dispatcher again (`ticket.assign`
/// without an employee); in Todo an employee, overriding the dispatcher (AC2).
/// Only active employees (AC3), grouped as in the org chart.
Future<bool> chooseOwner(BuildContext context, TicketView ticket) async {
  final company = context.read<CompanyBloc>().state;
  final dispatcher = dispatcherName(company, ticket.projectId);
  final groups = pickerGroups(company.employees.where((e) => e.status == LifecycleStatus.active).toList());
  final backlog = ticket.column == ColumnKind.backlog;
  final picked = await showModalBottomSheet<(String?,)>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
        child: ListView(
          shrinkWrap: true,
          children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow(backlog ? 'Owner of #${ticket.number}' : 'Choose an employee for #${ticket.number}')),
            if (backlog)
              ListTile(
                key: const ValueKey('owner-dispatcher'),
                leading: const Icon(CupertinoIcons.shuffle),
                title: Text(dispatcher == null ? 'Dispatcher (none set)' : 'Dispatcher ($dispatcher)'),
                subtitle: Text(dispatcher == null ? 'You choose the owner once it starts.' : '$dispatcher chooses when it starts.', style: TextStyle(color: Tokens.label2)),
                trailing: ticket.ownerEmployeeId == null ? Icon(Icons.check, color: Tokens.accentText) : null,
                onTap: () => Navigator.pop(sheet, (null,)),
              ),
            for (final g in groups) ...[
              Padding(padding: const EdgeInsets.fromLTRB(16, 12, 16, 4), child: Eyebrow(g.title)),
              for (final e in g.employees)
                ListTile(
                  key: ValueKey('owner-${e.id}'),
                  leading: Monogram(e.name.isEmpty ? '?' : e.name.characters.first.toUpperCase(), size: 30),
                  title: Text(e.name),
                  subtitle: Text(e.role, style: TextStyle(color: Tokens.label2)),
                  trailing: ticket.ownerEmployeeId == e.id ? Icon(Icons.check, color: Tokens.accentText) : null,
                  onTap: () => Navigator.pop(sheet, (e.id,)),
                ),
            ],
          ],
        ),
      ),
    ),
  );
  if (picked == null || !context.mounted || picked.$1 == ticket.ownerEmployeeId) return false;
  final id = picked.$1;
  final name = employeeName(company, id);
  return ticketWrite(
    context,
    ticketAssign(ticketId: ticket.id, employeeId: id),
    expectedVersion: ticket.version,
    done: id == null ? 'The dispatcher chooses the owner of #${ticket.number}.' : '$name owns #${ticket.number}.',
  );
}

/// The sheet "Responsible for #42" (§2.1): the explanation, You, the active
/// board members and, when someone is responsible, No one. Null: dismissed;
/// otherwise the choice, whose id is null for No one.
Future<(String?,)?> showResponsiblePicker(BuildContext context, {required String title, required String? current, required List<UserView> members, required String? me, bool noOne = true}) {
  final choices = responsibleChoices(members, me: me);
  Widget? tick(String? id) => current == id ? Icon(Icons.check, color: Tokens.accentText) : null;
  return showModalBottomSheet<(String?,)>(
    context: context,
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.75),
        child: ListView(shrinkWrap: true, children: [
          Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow(title)),
          Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 8), child: Text(responsibleExplained, style: TextStyle(color: Tokens.label2))),
          for (final c in choices)
            ListTile(
              key: ValueKey('responsible-${c.id}'),
              leading: Monogram(c.label.characters.first.toUpperCase(), size: 30, member: true),
              title: Text(c.label),
              trailing: tick(c.id),
              onTap: () => Navigator.pop(sheet, (c.id,)),
            ),
          if (noOne)
            ListTile(
              key: const ValueKey('responsible-no-one'),
              leading: const Icon(CupertinoIcons.circle),
              title: const Text('No one'),
              trailing: tick(null),
              onTap: () => Navigator.pop(sheet, (null,)),
            ),
        ]),
      ),
    ),
  );
}

/// Set responsible… (§2.2): any board member, at any moment but on Done (US-A1).
/// It takes effect at once; changing it again is the way back (no Undo, README).
Future<bool> chooseResponsible(BuildContext context, TicketView ticket) async {
  final link = context.read<ConnectionBloc>().state;
  final List<UserView> members;
  try {
    members = await context.read<AureliusRepository>().boardMembers();
  } on Object catch (e) {
    if (context.mounted) toast(context, ticketFailure(e));
    return false;
  }
  if (!context.mounted) return false;
  final picked = await showResponsiblePicker(context, title: 'Responsible for #${ticket.number}', current: ticket.responsibleUserId, members: members, me: link.user?.id, noOne: ticket.responsibleUserId != null);
  if (picked == null || picked.$1 == ticket.responsibleUserId || !context.mounted) return false;
  final name = members.where((u) => u.id == picked.$1).firstOrNull?.username;
  return setResponsible(context, ticket, picked.$1, name: name);
}

/// The write behind every way to change who is responsible, [userId] null for
/// no one. A handover has no version check on the server: the last one wins.
Future<bool> setResponsible(BuildContext context, TicketView ticket, String? userId, {String? name}) {
  final me = context.read<ConnectionBloc>().state.user?.id;
  return ticketWrite(
    context,
    ticketSetResponsible(ticketId: ticket.id, responsibleUserId: userId),
    done: switch (userId) {
      null => 'No one is responsible for #${ticket.number} now.',
      _ when userId == me => 'You are now responsible for #${ticket.number}.',
      _ => '${name ?? 'They'} is now responsible for #${ticket.number}.',
    },
  );
}

/// Follow or unfollow [ticket] (A2, §3.3). The server refuses to unfollow for
/// its active responsible (B10) and says why; the toast shows that.
Future<bool> followTicket(BuildContext context, TicketView ticket, {required bool on}) => ticketWrite(
      context,
      ticketFollow(FollowTicket(on: on, ticketId: ticket.id)),
      done: on ? "You'll get updates on #${ticket.number}." : 'Unfollowed #${ticket.number}. A new @mention or becoming responsible makes you follow again.',
    );

/// Whether [me] follows [ticket]; null when that could not be read.
Future<bool?> followsTicket(AureliusRepository repository, TicketView ticket, {String? me}) async {
  try {
    return (await repository.followers(ticket.id)).followers.any((f) => f.userId == me);
  } on Object {
    return null;
  }
}

/// A screen reader's Follow on a card: what the menu's item does, without its sheet.
Future<void> toggleFollow(BuildContext context, TicketView ticket) async {
  final me = context.read<ConnectionBloc>().state.user?.id;
  final following = await followsTicket(context.read<AureliusRepository>(), ticket, me: me);
  if (!context.mounted) return;
  if (following == null) return toast(context, 'Could not read who follows #${ticket.number}.');
  if (following && !canUnfollow(ticket, me: me)) return toast(context, followingNote(ticket, me: me));
  await followTicket(context, ticket, on: !following);
}

/// The sheet Followers (§3.2): names and why; only board members follow, never
/// employees. A disabled one stays listed and gets nothing.
Future<void> showFollowers(BuildContext context, TicketView ticket, List<FollowerView> followers) => showModalBottomSheet<void>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) => SafeArea(
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(sheet).height * 0.7),
          child: ListView(shrinkWrap: true, children: [
            Padding(padding: const EdgeInsets.fromLTRB(16, 0, 16, 4), child: Eyebrow('Followers of #${ticket.number}')),
            if (followers.isEmpty) Padding(padding: const EdgeInsets.fromLTRB(16, 8, 16, 16), child: Text('No one follows this ticket.', style: TextStyle(color: Tokens.label2))),
            for (final f in followers)
              ListTile(
                key: ValueKey('follower-${f.userId}'),
                leading: Monogram(f.username.isEmpty ? '?' : f.username.characters.first.toUpperCase(), size: 30, member: true, dimmed: f.disabled),
                title: Text(f.disabled ? '${f.username} (disabled)' : f.username),
                subtitle: followReasonLabel(f.reason) == null ? null : Text(followReasonLabel(f.reason)!, style: TextStyle(color: Tokens.label2)),
              ),
          ]),
        ),
      ),
    );

/// "Send #40 back to Doing" (§9.2, MU1-16 AC2/AC3): a full-height sheet with
/// the feedback field in focus; Send back stays off until there is text.
Future<bool> showSendBack(BuildContext context, TicketView ticket) async {
  final owner = ticket.ownerName ?? 'The owner';
  final feedback = await showModalBottomSheet<String>(
    context: context,
    useRootNavigator: true,
    isScrollControlled: true,
    useSafeArea: true,
    builder: (_) => FeedbackSheet(
      title: 'Send #${ticket.number} back to Doing',
      explain: '$owner gets your feedback and continues in the same conversation. The ticket returns to Review with a new report.',
      action: 'Send back',
    ),
  );
  if (feedback == null || !context.mounted) return false;
  return ticketWrite(context, ticketSendBack(ticketId: ticket.id, feedback: feedback), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is back in Doing with your feedback.');
}

/// A required text for a decision (B2): the action stays off until it is not
/// empty; submitting without text puts the error under the field.
class FeedbackSheet extends StatefulWidget {
  const FeedbackSheet({super.key, required this.title, required this.explain, required this.action, this.label = 'Feedback'});

  final String title;
  final String explain;
  final String action;
  final String label;

  @override
  State<FeedbackSheet> createState() => _FeedbackSheetState();
}

class _FeedbackSheetState extends State<FeedbackSheet> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  bool _tried = false;

  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _send() {
    final text = _text.text.trim();
    if (text.isEmpty) {
      setState(() => _tried = true);
      _focus.requestFocus();
      return;
    }
    Navigator.pop(context, text);
  }

  @override
  Widget build(BuildContext context) {
    final empty = _text.text.trim().isEmpty;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, meta: true): _send,
        const SingleActivator(LogicalKeyboardKey.enter, control: true): _send,
      },
      child: Padding(
        padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
        child: SizedBox(
          height: MediaQuery.sizeOf(context).height * 0.85,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(children: [
                TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
                Expanded(child: Text(widget.title, textAlign: TextAlign.center, maxLines: 2, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600))),
                TextButton(key: const ValueKey('feedback-send'), onPressed: empty ? null : _send, child: Text(widget.action)),
              ]),
              Padding(padding: const EdgeInsets.fromLTRB(20, 8, 20, 12), child: Text(widget.explain, style: TextStyle(color: Tokens.label2, fontSize: 15))),
              Expanded(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: TextField(
                    key: const ValueKey('feedback-field'),
                    controller: _text,
                    focusNode: _focus,
                    autofocus: true,
                    expands: true,
                    maxLines: null,
                    textAlignVertical: TextAlignVertical.top,
                    textCapitalization: TextCapitalization.sentences,
                    onSubmitted: (_) => _send(),
                    decoration: InputDecoration(
                      labelText: '${widget.label} *',
                      alignLabelWithHint: true,
                      helperText: 'Required.',
                      errorText: _tried && empty ? 'Write what needs to change.' : null,
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 12),
            ],
          ),
        ),
      ),
    );
  }
}

/// What holds Done (B6), as `ticket.accept` checks it: the linked tasks
/// still queued, running or waiting (the main task left out), and whether a run
/// goes or waits in the main task's conversation (a question asked in Review;
/// a ready or leased job, as the desktop). Null when either could not be read.
Future<({List<TaskView> tasks, bool conversation})?> doneHolds(AureliusRepository repository, TicketView ticket) async {
  try {
    final detail = await repository.ticket(ticket.id);
    final root = detail.rootTask ?? detail.tasks.where((t) => t.id == ticket.rootTaskId).firstOrNull;
    final conversation = root?.conversationId;
    final live = conversation != null && (await repository.jobs()).any((j) => j.conversationId == conversation && (j.state == JobState.ready || j.state == JobState.leased));
    return (tasks: detail.tasks.where((t) => t.stillWorks && t.id != ticket.rootTaskId).toList(), conversation: live);
  } on Object {
    return null;
  }
}

/// "Mark #40 as Done?" (§9.3, MU1-16 AC1/AC4, MU1-17): always a confirmation,
/// since Done cannot be undone. Done waits until nothing linked still works
/// (B6, [doneHolds], read fresh): the running tasks are listed and Done stays
/// off; also when that could not be read.
Future<bool> showMarkDone(BuildContext context, TicketView ticket) async {
  final holds = await doneHolds(context.read<AureliusRepository>(), ticket);
  if (!context.mounted) return false;
  final running = holds?.tasks ?? const <TaskView>[];
  final talking = holds?.conversation ?? false;
  final allowed = holds != null && running.isEmpty && !talking;
  final ok = await showDialog<bool>(
    context: context,
    builder: (dialog) => AlertDialog(
      title: Text('Mark #${ticket.number} as Done?'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text('Done closes the ticket. It cannot be reopened.\nNothing is merged, pushed or deployed.'),
          if (holds == null) ...[
            const SizedBox(height: 12),
            Text('⚠ Could not check whether anything still runs.', key: const ValueKey('mark-done-unknown'), style: TextStyle(color: Tokens.warning, fontWeight: FontWeight.w600)),
            const SizedBox(height: 6),
            const Text('Try again in a moment.'),
          ],
          if (running.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text('⚠ ${running.length == 1 ? '1 task is' : '${running.length} tasks are'} still running:', style: TextStyle(color: Tokens.warning, fontWeight: FontWeight.w600)),
            for (final t in running) Text('◌ ${t.title} — ${t.employeeName ?? 'nobody'} — ${t.statusLabel}', style: TextStyle(color: Tokens.label2)),
          ],
          if (talking) ...[
            const SizedBox(height: 12),
            Text('⚠ The owner is still answering in the conversation.', key: const ValueKey('mark-done-conversation'), style: TextStyle(color: Tokens.warning, fontWeight: FontWeight.w600)),
          ],
          if (running.isNotEmpty || talking) ...[
            const SizedBox(height: 6),
            const Text('Done is possible once nothing runs.'),
          ],
        ],
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
        FilledButton.icon(key: const ValueKey('mark-done'), onPressed: allowed ? () => Navigator.pop(dialog, true) : null, icon: const Icon(Icons.check), label: const Text('Done')),
      ],
    ),
  );
  if (ok != true || !context.mounted) return false;
  return ticketWrite(context, ticketAccept(ticketId: ticket.id), expectedVersion: ticket.version, retry: false, done: '#${ticket.number} is Done. A summary goes to project memory.');
}
