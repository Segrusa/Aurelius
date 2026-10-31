import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';

sealed class TicketEvent {
  const TicketEvent();
}

class TicketRefreshed extends TicketEvent {
  const TicketRefreshed();
}

class TicketPageState extends Equatable {
  const TicketPageState({this.loading = true, this.detail, this.error, this.activity = const [], this.activityFailed = false, this.plan, this.modeNow});

  final bool loading;
  final TicketDetail? detail;
  final String? error;

  /// The ticket's timeline, newest first (Activity, and whether the dispatcher gave up).
  final List<TimelineCard> activity;

  /// The last read of the activity failed: whether the dispatcher gave up is unknown (M-m3).
  final bool activityFailed;

  /// In Plan review: the plan the owner handed in (markdown).
  final String? plan;

  /// After a plan was approved: the mode of the main task's conversation
  /// (Auto or Accept edits), so Mode reads "Plan → Auto" (D-07). Null otherwise.
  final AgentMode? modeNow;

  TicketView? get ticket => detail?.ticket;
  TaskView? get root => detail?.rootTask;
  bool get gaveUp => dispatcherGaveUp(activity);

  @override
  List<Object?> get props => [loading, detail?.asOfSeq, detail?.ticket.version, detail?.tasks.map((t) => (t.id, t.status, t.updatedAtMs)).toList(), error, activity.map((c) => c.seq).toList(), activityFailed, plan, modeNow];
}

/// One ticket (`ticket.get`), its activity and, in Plan review, its plan;
/// read again on every change of the board.
class TicketBloc extends Bloc<TicketEvent, TicketPageState> {
  TicketBloc(this._repository, this.ticketId) : super(const TicketPageState()) {
    on<TicketRefreshed>(_refresh, transformer: debounceRestartable());
    _subs = [
      _repository.online.listen((_) => add(const TicketRefreshed())),
      _repository.boardChanges.listen((_) => add(const TicketRefreshed())),
    ];
    add(const TicketRefreshed());
  }

  final AureliusRepository _repository;
  final String ticketId;
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(TicketRefreshed event, Emitter<TicketPageState> emit) async {
    try {
      final detail = await _repository.ticket(ticketId);
      var activity = state.activity;
      var activityFailed = false;
      try {
        activity = (await _repository.timeline(correlationId: ticketId, activity: true, limit: 60)).cards;
      } on Object {
        // The ticket still shows; the page says what is unknown.
        activityFailed = true;
      }
      emit(TicketPageState(loading: false, detail: detail, activity: activity, activityFailed: activityFailed, plan: await _plan(detail), modeNow: await _modeNow(detail, activity)));
    } on DisconnectedException catch (e) {
      emit(TicketPageState(loading: false, detail: state.detail, activity: state.activity, activityFailed: state.activityFailed, plan: state.plan, modeNow: state.modeNow, error: state.detail == null ? e.message : null));
    } on ProtocolException catch (e) {
      emit(TicketPageState(loading: false, detail: state.detail, activity: state.activity, activityFailed: state.activityFailed, plan: state.plan, modeNow: state.modeNow, error: e.shown));
    }
  }

  /// The plan sits in the main task's conversation (S4): the last update of
  /// kind `plan`, else (an answer in Plan mode without ExitPlanMode) its last answer.
  Future<String?> _plan(TicketDetail detail) async {
    final conversation = detail.rootTask?.conversationId;
    if (detail.ticket.column != ColumnKind.planReview || conversation == null) return null;
    try {
      final messages = (await _repository.messages(conversation)).messages;
      final plan = messages.lastWhere((m) => m.update == 'plan', orElse: () => messages.lastWhere((m) => m.role == MessageRole.assistant && m.update == null, orElse: () => messages.last));
      return plan.text;
    } on Object {
      return null;
    }
  }

  /// The approval of a plan switches the main task's conversation out of Plan
  /// mode (`ticket.approve_plan`); the ticket keeps Plan. Read only once the
  /// timeline shows the approval; null when it cannot be read.
  Future<AgentMode?> _modeNow(TicketDetail detail, List<TimelineCard> activity) async {
    final conversation = detail.rootTask?.conversationId;
    if (detail.ticket.run.mode != AgentMode.plan || conversation == null || !activity.any((c) => c.title == 'ticket.plan_approved')) return null;
    try {
      final mode = (await _repository.messages(conversation)).agentMode;
      return mode == AgentMode.plan ? null : mode;
    } on Object {
      return state.modeNow;
    }
  }

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
