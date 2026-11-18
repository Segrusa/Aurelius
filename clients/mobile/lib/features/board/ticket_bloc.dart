import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/channels.dart' show ticketChannelsFeature;
import '../../data/tickets.dart';
import '../../data/views.dart';
import 'ticket_session_rules.dart';

sealed class TicketEvent {
  const TicketEvent();
}

class TicketRefreshed extends TicketEvent {
  const TicketRefreshed();
}

class TicketPageState extends Equatable {
  const TicketPageState({this.loading = true, this.detail, this.error, this.activity = const [], this.activityFailed = false, this.plan, this.modeNow, this.followers, this.followersFailed = false, this.notFound = false, this.denied = false, this.number, this.stopped});

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

  /// Who follows the ticket; null on a server without `ticket_follow`, or until read.
  final List<FollowerView>? followers;

  /// The last read of [followers] failed: the page says so and offers to try again.
  final bool followersFailed;

  /// The server knows no such ticket (a link to one that is gone, §6.4).
  final bool notFound;

  /// The server refuses this reader the ticket.
  final bool denied;

  /// The number in the route (`/ticket/70`), for "Ticket #70 not found."; null for an id.
  final int? number;

  /// Doing, and the main task stopped for input ([mainTaskStopped]): its last
  /// failure, if it failed, and the question Try again sends again. Null otherwise.
  final ({String? failure, String retry})? stopped;

  TicketView? get ticket => detail?.ticket;
  TaskView? get root => detail?.rootTask;
  bool get gaveUp => dispatcherGaveUp(activity);

  @override
  List<Object?> get props => [loading, detail?.asOfSeq, detail?.ticket.version, detail?.ticket.responsibleUserId, detail?.tasks.map((t) => (t.id, t.status, t.updatedAtMs)).toList(), error, activity.map((c) => c.seq).toList(), activityFailed, plan, modeNow, followers?.map((f) => (f.userId, f.reason, f.disabled)).toList(), followersFailed, notFound, denied, number, stopped];
}

/// One ticket (`ticket.get`), its activity, its followers and, in Plan review,
/// its plan; read again on every change of the board and of who follows it
/// (`ticket.follow_changed`).
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

  /// The ticket's id, or its number as the route has it (`/ticket/70`).
  final String ticketId;

  /// The id [ticketId] resolved to; the same as it unless it is a number.
  String? _id;
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(TicketRefreshed event, Emitter<TicketPageState> emit) async {
    final number = routeNumber(ticketId);
    try {
      final id = _id ??= await _resolve(number);
      if (id == null) {
        emit(TicketPageState(loading: false, notFound: true, number: number));
        return;
      }
      final detail = await _repository.ticket(id);
      var activity = state.activity;
      var activityFailed = false;
      try {
        activity = (await _repository.timeline(correlationId: id, activity: true, limit: 60)).cards;
      } on Object {
        // The ticket still shows; the page says what is unknown.
        activityFailed = true;
      }
      final (followers, followersFailed) = await _followers(id);
      emit(TicketPageState(loading: false, detail: detail, activity: activity, activityFailed: activityFailed, plan: await _plan(detail), modeNow: await _modeNow(detail, activity), followers: followers, followersFailed: followersFailed, number: number, stopped: await _stopped(detail)));
    } on DisconnectedException catch (e) {
      emit(_kept(error: state.detail == null ? e.message : null, number: number));
    } on ProtocolException catch (e) {
      emit(_kept(error: e.shown, notFound: e.code == ErrorCode.notFound, denied: e.isForbidden, number: number));
    }
  }

  /// The last good read, with what went wrong since.
  TicketPageState _kept({String? error, bool notFound = false, bool denied = false, int? number}) => TicketPageState(loading: false, detail: state.detail, activity: state.activity, activityFailed: state.activityFailed, plan: state.plan, modeNow: state.modeNow, followers: state.followers, followersFailed: state.followersFailed, error: error, notFound: notFound, denied: denied, number: number, stopped: state.stopped);

  /// The id of the ticket the route names: an id as it is; a number through
  /// the ticket channels (`channel.search "#70"`, archived ones too). Null:
  /// no such number, or a server without ticket channels cannot say.
  Future<String?> _resolve(int? number) async {
    if (number == null) return ticketId;
    if (!_repository.linkState.has(ticketChannelsFeature)) return null;
    final found = await _repository.searchChannels('#$number');
    return found.where((c) => c.ticketNumber == number && c.ticketId != null).firstOrNull?.ticketId;
  }

  /// A main task that stopped for input: its last failure and what Try again
  /// sends (as the desktop's status box); the last known when its conversation cannot be read.
  Future<({String? failure, String retry})?> _stopped(TicketDetail detail) async {
    final conversation = detail.rootTask?.conversationId;
    if (!mainTaskStopped(detail.ticket, detail.rootTask) || conversation == null) return null;
    try {
      final messages = (await _repository.messages(conversation)).messages;
      return (failure: lastFailure(messages), retry: retryText(messages));
    } on Object {
      return state.stopped;
    }
  }

  /// Who follows the ticket, when the server keeps that; on a failure the last
  /// known list stays, marked as failed.
  Future<(List<FollowerView>?, bool)> _followers(String id) async {
    if (!_repository.linkState.has(ticketFollowFeature)) return (null, false);
    try {
      return ((await _repository.followers(id)).followers, false);
    } on Object {
      return (state.followers, true);
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
