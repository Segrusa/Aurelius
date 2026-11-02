import 'dart:async';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/guidelines.dart';
import '../../data/links.dart';
import '../../data/secret_write.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';
import 'inbox_rows.dart';

sealed class InboxEvent {
  const InboxEvent();
}

class InboxRefreshed extends InboxEvent {
  const InboxRefreshed();
}

/// Approve or reject exactly this action (digest included). Never retried
/// later on its own.
class ApprovalDecided extends InboxEvent {
  const ApprovalDecided(this.approval, {required this.approve, this.alwaysAllow = false});

  final ApprovalView approval;
  final bool approve;

  /// Approve and always allow this MCP tool from now on (`alwaysAllowEligible` only).
  final bool alwaysAllow;
}

/// The server approved a pending ask because its MCP tool became always
/// allowed (`approval.allowed_always`): it leaves the list at once.
class ApprovalAllowedAlways extends InboxEvent {
  const ApprovalAllowedAlways(this.approvalId);

  final String approvalId;
}

/// The user's answers to Claude's question (see `Commands.answerQuestion`).
class QuestionAnswered extends InboxEvent {
  const QuestionAnswered(this.approval, this.answers, {this.ask});

  final ApprovalView approval;
  final Map<String, String> answers;

  /// Asks about a vault value in an own answer (B10); without it a refusal is a notice.
  final SendAnywayAsk? ask;
}

/// The user's decision on Claude's plan. Revise needs feedback; reject may have some.
class PlanDecided extends InboxEvent {
  const PlanDecided(this.approval, this.decision, {this.feedback, this.ask});

  final ApprovalView approval;
  final PlanDecision decision;
  final String? feedback;

  /// Asks about a vault value in the feedback (B10); without it a refusal is a notice.
  final SendAnywayAsk? ask;
}

/// How a plan ended up, for the status the plan sheet shows after a choice.
enum PlanOutcome { approved, approvedEdits, revisionRequested, rejected, expired, handledElsewhere }

/// A task was opened: its result notifications count as read (dismissed), so
/// finished work never lingers in the inbox after you have looked at it.
class TaskOpened extends InboxEvent {
  const TaskOpened(this.taskId);

  final String taskId;
}

class NotificationsMarkedRead extends InboxEvent {
  const NotificationsMarkedRead();
}

/// A row of the personal Inbox was opened: only that makes it read (§4.4). There
/// is no read per row on the server yet, so this phone remembers it until the
/// next event on the row, and sends no mark-all while others are unread.
class InboxRowOpened extends InboxEvent {
  const InboxRowOpened(this.row);

  final NotificationView row;
}

/// A notification was swiped away.
class NotificationDismissed extends InboxEvent {
  const NotificationDismissed(this.id);

  final String id;
}

/// Clear every result (not approvals): `notification.dismiss_all`.
class NotificationsCleared extends InboxEvent {
  const NotificationsCleared();
}

/// Approve or decline a colleague or new instructions an employee proposed
/// (needs the Approve right).
class ProposalDecided extends InboxEvent {
  const ProposalDecided(this.proposal, {required this.approve, this.note, this.instructions});

  final ProposalView proposal;
  final bool approve;
  final String? note;

  /// Proposed instructions approved with the user's edits.
  final String? instructions;
}

/// Load (again) a task's variants for its compare card (`task.variants`).
class VariantsRequested extends InboxEvent {
  const VariantsRequested(this.taskId);

  final String taskId;
}

/// Take one of a task's variants; [variant] null takes the judge's pick.
class VariantChosen extends InboxEvent {
  const VariantChosen(this.taskId, {this.variant});

  final String taskId;
  final int? variant;
}

class InboxNoticeShown extends InboxEvent {
  const InboxNoticeShown();
}

/// Read the pending guideline proposals again (feature `guidelines`): at the
/// start, when the link is back and on a `guideline.*` event only, not on
/// every change of the company.
class GuidelineProposalsRefreshed extends InboxEvent {
  const GuidelineProposalsRefreshed();
}

/// Approve or reject a guideline proposal (B5, GL-14). On approve [title] and
/// [text] are the board member's edits (null: as proposed) and [links] the
/// links of a new guideline (null: the suggested ones). [names] words the
/// refusal of B4.
class GuidelineProposalDecided extends InboxEvent {
  const GuidelineProposalDecided(this.item, {required this.approve, this.note, this.title, this.text, this.links, required this.proposer, this.employee, this.project});

  final GuidelineProposalItem item;
  final bool approve;
  final String? note;
  final String? title;
  final String? text;
  final List<GuidelineTarget>? links;

  /// The proposer's name, for the notice.
  final String proposer;
  final String Function(String id)? employee;
  final String Function(String id)? project;
}

class InboxState extends Equatable {
  const InboxState({
    this.loading = true,
    this.approvals = const [],
    this.proposals = const [],
    this.guidelineProposals = const [],
    this.notifications = const [],
    this.unread = 0,
    this.deciding = const {},
    this.planOutcomes = const {},
    this.variants = const {},
    this.variantsOn = false,
    this.personalOn = false,
    this.followOn = false,
    this.linksOn = false,
    this.loadFailed = false,
    this.notice,
    this.notSent,
  });

  final bool loading;

  /// Pending approvals only.
  final List<ApprovalView> approvals;

  /// Colleagues employees proposed that wait for a decision.
  final List<ProposalView> proposals;

  /// Guideline proposals that wait for a board member (B5).
  final List<GuidelineProposalItem> guidelineProposals;
  final List<NotificationView> notifications;
  final int unread;

  /// Approval ids with a decision on its way.
  final Set<String> deciding;

  /// Plans decided here (or found handled elsewhere), by approval id.
  final Map<String, PlanOutcome> planOutcomes;

  /// Loaded variant comparisons, by task id.
  final Map<String, TaskVariantsView> variants;

  /// The server has `task_variants`: tasks with variants ready show a compare card.
  final bool variantsOn;

  /// The server has `personal_inbox`: rows per ticket or channel (A3).
  final bool personalOn;

  /// The server has `ticket_follow`: a row of a ticket offers Unfollow.
  final bool followOn;

  /// The server has `message_locate`: a row offers Copy link.
  final bool linksOn;

  /// The personal Inbox could not be read: it says so, with Try again (§4.6).
  final bool loadFailed;
  final String? notice;

  /// The approval whose answer or decision the board member just chose not to
  /// send (a vault value, B10): it stays open to change.
  final String? notSent;

  /// The rows of the personal Inbox; none on a server without it.
  List<NotificationView> get personalRows => personalOn ? notifications.where((n) => n.isPersonal).toList() : const [];

  /// The badge: decisions waiting, plus unread notifications that need you or
  /// say a task failed, plus the rows of the personal Inbox that are unread or
  /// wait for you. Results (finished work) give no badge.
  int get badge =>
      approvals.length + proposals.length + guidelineProposals.length + notifications.where((n) => n.givesBadge || (personalOn && n.isPersonal && (!n.read || n.waitsForMe))).length;

  ApprovalView? approval(String id) => approvals.where((a) => a.id == id).firstOrNull;

  /// The pending approval that blocks a conversation's run, if any.
  ApprovalView? blocking(String? conversationId) =>
      conversationId == null ? null : approvals.where((a) => a.conversationId == conversationId).firstOrNull;

  /// What a conversation waits for from you, for its row in a list: "Answer",
  /// "Review plan" or "Approve"; null when nothing.
  String? needsYou(String conversationId) {
    final waiting = approvals.where((a) => a.conversationId == conversationId);
    if (waiting.any((a) => a.isQuestion)) return 'Answer';
    if (waiting.any((a) => a.isPlan)) return 'Review plan';
    return waiting.isEmpty ? null : 'Approve';
  }

  InboxState copyWith({
    bool? loading,
    List<ApprovalView>? approvals,
    List<ProposalView>? proposals,
    List<GuidelineProposalItem>? guidelineProposals,
    List<NotificationView>? notifications,
    int? unread,
    Set<String>? deciding,
    Map<String, PlanOutcome>? planOutcomes,
    Map<String, TaskVariantsView>? variants,
    bool? variantsOn,
    bool? personalOn,
    bool? followOn,
    bool? linksOn,
    bool? loadFailed,
    String? notice,
    String? notSent,
  }) =>
      InboxState(
        loading: loading ?? this.loading,
        approvals: approvals ?? this.approvals,
        proposals: proposals ?? this.proposals,
        guidelineProposals: guidelineProposals ?? this.guidelineProposals,
        notifications: notifications ?? this.notifications,
        unread: unread ?? this.unread,
        deciding: deciding ?? this.deciding,
        planOutcomes: planOutcomes ?? this.planOutcomes,
        variants: variants ?? this.variants,
        variantsOn: variantsOn ?? this.variantsOn,
        personalOn: personalOn ?? this.personalOn,
        followOn: followOn ?? this.followOn,
        linksOn: linksOn ?? this.linksOn,
        loadFailed: loadFailed ?? this.loadFailed,
        notice: notice,
        notSent: notSent,
      );

  @override
  List<Object?> get props => [loading, approvals, proposals, guidelineProposals, notifications, unread, deciding, planOutcomes, variants, variantsOn, personalOn, followOn, linksOn, loadFailed, notice, notSent];
}

extension NotificationAttention on NotificationView {
  /// Alerts whose approval or proposal is listed (and counted) on its own.
  static const _listedItself = {'approval.requested', 'employee.proposed', 'employee.instructions_proposed', 'guideline.proposed'};

  bool get isFailure => targetType == 'task.failed';

  /// Needs you: an alert for something to act on that is not already a listed approval or proposal.
  bool get asksYou => severity == 'action' && !_listedItself.contains(targetType);

  /// Only Needs you and Failed, unread, give the badge; a finished result does not.
  bool get givesBadge => !read && (isFailure || asksYou);
}

/// Approvals and notifications for the active PC; also drives the badge.
class InboxBloc extends Bloc<InboxEvent, InboxState> {
  InboxBloc(this._repository) : super(const InboxState()) {
    on<InboxRefreshed>(_refresh, transformer: debounceRestartable());
    on<ApprovalDecided>(_decide, transformer: concurrent());
    on<ApprovalAllowedAlways>((e, emit) => emit(state.copyWith(approvals: state.approvals.where((a) => a.id != e.approvalId).toList(), deciding: state.deciding)));
    on<QuestionAnswered>(_answer, transformer: concurrent());
    on<PlanDecided>(_decidePlan, transformer: concurrent());
    on<NotificationsMarkedRead>(_markRead, transformer: droppable());
    on<TaskOpened>(_taskOpened, transformer: sequential());
    on<InboxRowOpened>(_rowOpened);
    on<NotificationDismissed>(_dismiss);
    on<NotificationsCleared>(_clearAll, transformer: droppable());
    on<ProposalDecided>(_decideProposal, transformer: concurrent());
    on<VariantsRequested>(_loadVariants, transformer: concurrent());
    on<VariantChosen>(_chooseVariant, transformer: concurrent());
    on<InboxNoticeShown>((e, emit) => emit(state.copyWith()));
    on<GuidelineProposalsRefreshed>(_refreshGuidelineProposals, transformer: debounceRestartable());
    on<GuidelineProposalDecided>(_decideGuidelineProposal, transformer: concurrent());
    _subs = [
      _repository.online.listen((_) {
        add(const InboxRefreshed());
        add(const GuidelineProposalsRefreshed());
      }),
      _repository.companyChanges.listen((e) {
        if (e.kind == EventKind.approvalAllowedAlways) add(ApprovalAllowedAlways(e.aggregateId));
        if (e.kind.wire.startsWith('guideline.')) add(const GuidelineProposalsRefreshed());
        if (e.kind.wire.startsWith('task.variant')) add(VariantsRequested(e.aggregateId));
        add(const InboxRefreshed());
      }),
    ];
    if (_repository.linkState.isOnline) {
      add(const InboxRefreshed());
      add(const GuidelineProposalsRefreshed());
    }
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<void>> _subs;

  /// A mention is a chat event: only the personal Inbox needs those, so they are listened to once it is there.
  StreamSubscription<void>? _chat;

  /// Rows opened here, with the time of the event they were at: a later event makes them unread again.
  final _opened = <String, int>{};

  bool _isOpened(NotificationView n) => _opened[n.id] == n.createdAtMs;

  Future<void> _refresh(InboxRefreshed event, Emitter<InboxState> emit) async {
    try {
      final approvals = await _repository.approvals();
      final notifications = await _repository.notifications();
      final proposals = await _repository.proposals();
      approvals.sort((a, b) => a.expiresAtMs.compareTo(b.expiresAtMs));
      final link = _repository.linkState;
      final personal = link.has(personalInboxFeature);
      if (personal) _chat ??= _repository.chatChanges.listen((_) => add(const InboxRefreshed()));
      emit(state.copyWith(
        loading: false,
        approvals: approvals.where((a) => a.isPending).toList(),
        // Proposed instructions only where the server has the feature.
        proposals: proposals.where((p) => p.isPending && (!p.isInstructions || link.has(Harness2.instructionProposals))).toList(),
        variantsOn: link.has(Harness2.variants),
        personalOn: personal,
        followOn: link.has(ticketFollowFeature),
        linksOn: link.has(messageLocateFeature),
        loadFailed: false,
        notifications: [for (final n in notifications.notifications) _isOpened(n) ? readRow(n) : n],
        unread: (notifications.unread - notifications.notifications.where((n) => !n.read && _isOpened(n)).length).clamp(0, 1 << 30),
        deciding: state.deciding,
      ));
    } on DisconnectedException {
      emit(state.copyWith(loading: false, deciding: state.deciding));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, notice: e.message, loadFailed: _repository.linkState.has(personalInboxFeature), deciding: state.deciding));
    }
  }

  Future<void> _decide(ApprovalDecided event, Emitter<InboxState> emit) async {
    final id = event.approval.id;
    emit(state.copyWith(deciding: {...state.deciding, id}));
    String? notice;
    try {
      final receipt = await _repository.write(Commands.decideApproval(id, event.approval.actionDigest, event.approve, alwaysAllow: event.alwaysAllow), retry: false);
      final status = switch (receipt.command) { CommandResultApprovalDecided(:final status) => status, _ => null };
      notice = switch (status) {
        ApprovalStatus.consumed || ApprovalStatus.approved when event.alwaysAllow =>
          'Approved. ${event.approval.mcpToolLabel} is always allowed from now on.',
        ApprovalStatus.consumed || ApprovalStatus.approved => 'Approved. Only this exact action runs.',
        ApprovalStatus.rejected when event.approval.isQuestion => 'Skipped. Claude continues on its own judgment.',
        ApprovalStatus.rejected => 'Rejected.',
        _ => 'This request changed or was already handled.',
      };
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was decided.';
    } on ProtocolException catch (e) {
      // Expired, changed, or decided elsewhere (the desktop, another phone);
      // an always allow the server refuses says why.
      notice = event.alwaysAllow && e.code == ErrorCode.validation && e.message.isNotEmpty
          ? e.message
          : e.code == ErrorCode.validation || e.code == ErrorCode.notFound ? 'This request changed or was already handled.' : e.message;
    }
    emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: notice));
    add(const InboxRefreshed());
  }

  Future<void> _answer(QuestionAnswered event, Emitter<InboxState> emit) async {
    final id = event.approval.id;
    emit(state.copyWith(deciding: {...state.deciding, id}));
    String? notice;
    try {
      final receipt = await _write(Commands.answerQuestion(id, event.answers), event.ask);
      if (receipt == null) {
        emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: 'Not sent. Nothing was answered.', notSent: id));
        return;
      }
      final status = switch (receipt.command) { CommandResultApprovalDecided(:final status) => status, _ => null };
      notice = status == ApprovalStatus.consumed ? 'Answer sent.' : 'This question expired; Claude continues on its own judgment.';
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was answered.';
    } on ProtocolException catch (e) {
      // Refused again after Send anyway: it stays to change, no new question.
      if (e.looksLikeSecret) {
        emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: e.message, notSent: id));
        return;
      }
      notice = e.code == ErrorCode.validation || e.code == ErrorCode.notFound ? 'This question changed or was already answered.' : e.message;
    }
    emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: notice));
    add(const InboxRefreshed());
  }

  /// A decision with the board member's own text: with [ask], a vault value
  /// in it asks first and Send anyway sends it once more (B10). Null: not sent.
  Future<Receipt?> _write(Map<String, dynamic> request, SendAnywayAsk? ask) =>
      ask == null ? _repository.write(request, retry: false) : writeConfirmingSecret(_repository, request, ask, retry: false);

  Future<void> _decidePlan(PlanDecided event, Emitter<InboxState> emit) async {
    final id = event.approval.id;
    final feedback = event.feedback?.trim();
    if (event.decision == PlanDecision.revise && (feedback == null || feedback.isEmpty)) {
      emit(state.copyWith(notice: 'Say what should change.'));
      return;
    }
    emit(state.copyWith(deciding: {...state.deciding, id}));
    String? notice;
    PlanOutcome? outcome;
    try {
      final receipt = await _write(Commands.decidePlan(id, event.decision, feedback: feedback), event.ask);
      if (receipt == null) {
        emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: 'Not sent. Nothing was decided.', notSent: id));
        return;
      }
      final status = switch (receipt.command) { CommandResultApprovalDecided(:final status) => status, _ => null };
      if (status == ApprovalStatus.expired) {
        outcome = PlanOutcome.expired;
        notice = 'This plan expired; nothing was approved.';
      } else {
        (outcome, notice) = switch (event.decision) {
          PlanDecision.approve => (PlanOutcome.approved, 'Plan approved. The work goes ahead in auto mode.'),
          PlanDecision.approveEdits => (PlanOutcome.approvedEdits, 'Plan approved. Edits go through; the worker asks for the rest.'),
          PlanDecision.revise => (PlanOutcome.revisionRequested, 'Changes requested. The plan is revised.'),
          PlanDecision.reject => (PlanOutcome.rejected, 'Plan rejected. The work stops.'),
        };
      }
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was decided.';
    } on ProtocolException catch (e) {
      // Refused again after Send anyway: it stays to change, no new question.
      if (e.looksLikeSecret) {
        emit(state.copyWith(deciding: {...state.deciding}..remove(id), notice: e.message, notSent: id));
        return;
      }
      // Decided elsewhere (the desktop, another phone) or gone.
      final gone = e.code == ErrorCode.validation || e.code == ErrorCode.notFound;
      if (gone) outcome = PlanOutcome.handledElsewhere;
      notice = gone ? 'This plan was already handled.' : e.message;
    }
    emit(state.copyWith(
      deciding: {...state.deciding}..remove(id),
      planOutcomes: outcome == null ? null : {...state.planOutcomes, id: outcome},
      notice: notice,
    ));
    add(const InboxRefreshed());
  }

  void _rowOpened(InboxRowOpened event, Emitter<InboxState> emit) {
    final row = event.row;
    _opened[row.id] = row.createdAtMs;
    if (row.read) return;
    emit(state.copyWith(
      notifications: [for (final n in state.notifications) n.id == row.id ? readRow(n) : n],
      unread: (state.unread - 1).clamp(0, 1 << 30),
      deciding: state.deciding,
    ));
  }

  Future<void> _dismiss(NotificationDismissed event, Emitter<InboxState> emit) async {
    final gone = state.notifications.where((n) => n.id == event.id).firstOrNull;
    emit(state.copyWith(
      notifications: state.notifications.where((n) => n.id != event.id).toList(),
      unread: gone != null && !gone.read ? (state.unread - 1).clamp(0, 1 << 30) : state.unread,
      deciding: state.deciding,
    ));
    try {
      await _repository.dismissNotification(event.id);
    } catch (_) {
      add(const InboxRefreshed());
    }
  }

  Future<void> _clearAll(NotificationsCleared event, Emitter<InboxState> emit) async {
    final loaded = state.notifications;
    if (loaded.isEmpty) return;
    // The inbox lists "action" alerts as the live items in Needs you, not as results.
    final personal = state.personalRows;
    final results = loaded.where((n) => n.severity != 'action' && !personal.contains(n)).toList();
    emit(state.copyWith(
      // Clear all is about Done: the rows of the personal Inbox stay.
      notifications: personal,
      unread: personal.where((n) => !n.read).length,
      deciding: state.deciding,
      notice: results.length == 1 ? 'Cleared 1 result.' : 'Cleared ${results.length} results.',
    ));
    try {
      if (state.personalOn) {
        for (final n in results) {
          await _repository.dismissNotification(n.id);
        }
      } else {
        await _repository.dismissAllNotifications();
      }
    } catch (_) {
      add(const InboxRefreshed());
    }
  }

  Future<void> _decideProposal(ProposalDecided event, Emitter<InboxState> emit) async {
    final p = event.proposal;
    final target = p.targetEmployeeName ?? 'the employee';
    String? notice;
    try {
      await _repository.write(Commands.decideProposal(p.id, event.approve, note: event.note, instructions: event.approve ? event.instructions : null), retry: false);
      notice = switch ((p.isInstructions, event.approve)) {
        (true, true) => event.instructions != null && event.instructions != p.instructions ? 'Your edited instructions for $target are saved.' : 'New instructions for $target are saved.',
        (true, false) => 'Rejected. $target keeps its instructions.',
        (false, true) => '${p.name} joined the project.',
        (false, false) => 'Declined.',
      };
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was decided.';
    } on ProtocolException catch (e) {
      notice = e.code == ErrorCode.versionConflict && p.isInstructions
          ? "$target's instructions changed after this proposal. Reject it; the employee can propose again."
          : e.message;
    }
    emit(state.copyWith(notice: notice));
    add(const InboxRefreshed());
  }

  Future<void> _refreshGuidelineProposals(GuidelineProposalsRefreshed event, Emitter<InboxState> emit) async {
    if (!_repository.linkState.has('guidelines')) {
      if (state.guidelineProposals.isNotEmpty) emit(state.copyWith(guidelineProposals: const [], deciding: state.deciding));
      return;
    }
    try {
      final pending = await _repository.guidelineProposalList(status: ProposalStatus.pending);
      final items = <GuidelineProposalItem>[];
      for (final p in pending.where((p) => p.status == ProposalStatus.pending)) {
        final id = p.guidelineId;
        if (id == null) {
          items.add(GuidelineProposalItem(proposal: p));
          continue;
        }
        // The diff is against the version the proposal rests on (GL-14 AC1).
        try {
          final base = await _repository.guideline(id, version: p.baseVersion);
          items.add(GuidelineProposalItem(proposal: p, current: base.guideline, baseText: base.version.text));
        } on ProtocolException {
          items.add(GuidelineProposalItem(proposal: p));
        }
      }
      emit(state.copyWith(guidelineProposals: items, deciding: state.deciding));
    } on DisconnectedException {
      // Read again when the link is back.
    } on ProtocolException catch (e) {
      if (!e.notOnServer) emit(state.copyWith(notice: e.shown, deciding: state.deciding));
    }
  }

  Future<void> _decideGuidelineProposal(GuidelineProposalDecided event, Emitter<InboxState> emit) async {
    final item = event.item;
    final p = item.proposal;
    final key = 'guideline-proposal:${p.id}';
    emit(state.copyWith(deciding: {...state.deciding, key}));
    final title = event.title ?? p.title;
    String? notice;
    try {
      await _repository.write(
        guidelineDecideProposal(
          proposalId: p.id,
          approve: event.approve,
          note: event.approve ? null : event.note,
          title: event.approve && event.title != null && event.title != p.title ? event.title : null,
          text: event.approve && event.text != null && event.text != p.text ? event.text : null,
          links: event.approve && item.isNew ? event.links : null,
        ),
        retry: false,
      );
      notice = switch ((event.approve, item.isNew)) {
        (true, true) => 'Approved. “$title” is in the library and applies from the next run.',
        (true, false) => 'Approved. The new version of “$title” applies from the next run.',
        (false, _) => event.note == null ? 'Rejected. ${event.proposer} is told.' : 'Rejected. ${event.proposer} gets your note.',
      };
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was decided.';
    } on ProtocolException catch (e) {
      // The proposal stays open with the reason (GL-14 AC3, AC5).
      notice = switch (e) {
        ProtocolException(code: ErrorCode.versionConflict) =>
          '“$title” changed since this was proposed, so it can\'t be approved. Reject it; ${event.proposer} can propose again on the new text.',
        _ => guidelineError(e, title: title, employee: event.employee ?? (_) => 'an employee', project: event.project ?? (_) => 'a project'),
      };
    }
    emit(state.copyWith(deciding: {...state.deciding}..remove(key), notice: notice));
    add(const GuidelineProposalsRefreshed());
  }

  Future<void> _loadVariants(VariantsRequested event, Emitter<InboxState> emit) async {
    if (!_repository.linkState.has(Harness2.variants)) return;
    try {
      final view = await _repository.variantsOf(event.taskId);
      emit(state.copyWith(variants: {...state.variants, event.taskId: view}));
    } on DisconnectedException {
      // Loaded again when the link is back.
    } on ProtocolException {
      // No variants (any more): forget what was loaded.
      if (state.variants.containsKey(event.taskId)) emit(state.copyWith(variants: {...state.variants}..remove(event.taskId)));
    }
  }

  Future<void> _chooseVariant(VariantChosen event, Emitter<InboxState> emit) async {
    final key = 'variants:${event.taskId}';
    emit(state.copyWith(deciding: {...state.deciding, key}));
    String? notice;
    try {
      final receipt = await _repository.write(Commands.chooseVariant(event.taskId, variant: event.variant), retry: false);
      final n = switch (receipt.command) { CommandResultTaskVariantChosen(:final variant) => variant, _ => event.variant };
      notice = event.variant == null ? "The judge's pick${n == null ? '' : ', variant $n,'} is applied." : 'Variant $n is applied.';
    } on NotSentException {
      notice = 'Not sent. The PC was not reachable; nothing was chosen.';
    } on ProtocolException catch (e) {
      notice = e.code == ErrorCode.validation || e.code == ErrorCode.notFound ? 'These variants were already settled.' : e.message;
    }
    emit(state.copyWith(deciding: {...state.deciding}..remove(key), notice: notice));
    add(VariantsRequested(event.taskId));
    add(const InboxRefreshed());
  }

  Future<void> _taskOpened(TaskOpened event, Emitter<InboxState> emit) async {
    // Alerts for something to act on stay until it is done; the rest was seen now.
    // A row of the personal Inbox is read by opening that row (§4.4).
    final seen = state.notifications.where((n) => n.taskId == event.taskId && n.severity != 'action' && !state.personalRows.contains(n)).toList();
    if (seen.isEmpty) return;
    final ids = {for (final n in seen) n.id};
    emit(state.copyWith(
      notifications: state.notifications.where((n) => !ids.contains(n.id)).toList(),
      unread: (state.unread - seen.where((n) => !n.read).length).clamp(0, 1 << 30),
      deciding: state.deciding,
    ));
    try {
      for (final id in ids) {
        await _repository.dismissNotification(id);
      }
    } catch (_) {
      add(const InboxRefreshed());
    }
  }

  Future<void> _markRead(NotificationsMarkedRead event, Emitter<InboxState> emit) async {
    if (state.unread == 0) return;
    // Seeing the list counts for Done only; mark-all would read these rows as well.
    if (state.personalRows.any((n) => !n.read)) return;
    try {
      await _repository.markNotificationsRead();
      emit(state.copyWith(unread: 0, notifications: state.notifications.map(readRow).toList(), deciding: state.deciding));
    } catch (_) {}
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    await _chat?.cancel();
    return super.close();
  }
}
