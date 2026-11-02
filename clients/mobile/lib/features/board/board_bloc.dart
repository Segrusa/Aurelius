import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/tickets.dart';
import '../../data/views.dart';

sealed class BoardEvent {
  const BoardEvent();
}

class BoardRefreshed extends BoardEvent {
  const BoardRefreshed();
}

/// Whether the Board tab is on screen. Hidden, changes only mark the board
/// stale; it is read again once the tab shows (a phone's battery and data).
class BoardShown extends BoardEvent {
  const BoardShown(this.visible);

  final bool visible;
}

/// The project filter (B4); null: every project.
class BoardProjectChosen extends BoardEvent {
  const BoardProjectChosen(this.projectId);

  final String? projectId;
}

/// The owner filter: null anyone, [BoardState.noOwner] tickets without one, else an employee.
class BoardOwnerChosen extends BoardEvent {
  const BoardOwnerChosen(this.owner);

  final String? owner;
}

/// The creator filter (ui §3 "Created by", R3 m6): null anyone, else a board member's id.
class BoardCreatorChosen extends BoardEvent {
  const BoardCreatorChosen(this.userId);

  final String? userId;
}

/// The responsible filter (P-77): null anyone, [BoardState.noResponsible] tickets without one, else a board member's id.
class BoardResponsibleChosen extends BoardEvent {
  const BoardResponsibleChosen(this.userId);

  final String? userId;
}

/// The filter Needs you (B6): only the tickets that count as your work. The
/// page applies it, since it takes the company around the ticket ([needsMe]).
class BoardNeedsMeOnly extends BoardEvent {
  const BoardNeedsMeOnly(this.on);

  final bool on;
}

/// The column the phone shows (the last one viewed is kept).
class BoardColumnShown extends BoardEvent {
  const BoardColumnShown(this.kind);

  final ColumnKind kind;
}

class BoardState extends Equatable {
  const BoardState({
    this.loading = true,
    this.board,
    this.error,
    this.unsupported = false,
    this.projectId,
    this.owner,
    this.createdBy,
    this.responsible,
    this.needsMeOnly = false,
    this.column,
    this.gaveUp = const {},
    this.ownerUnknown = const {},
  });

  /// [owner] value for "No owner yet".
  static const noOwner = '';

  /// [responsible] value for "No one".
  static const noResponsible = '';

  /// Until the first `board.get` answered.
  final bool loading;
  final BoardView? board;

  /// Why the board could not be read; the last known board stays shown.
  final String? error;

  /// The server has no Board yet (`unknown_method`).
  final bool unsupported;
  final String? projectId;
  final String? owner;

  /// Only tickets this board member created; null: anyone's.
  final String? createdBy;

  /// Only tickets this board member is responsible for; null: anyone's.
  final String? responsible;

  /// The filter Needs you (B6).
  final bool needsMeOnly;

  /// The column last shown on a phone; null until the user picks one.
  final ColumnKind? column;

  /// Tickets in Todo whose dispatcher could not choose an owner (B5).
  final Set<String> gaveUp;

  /// Tickets in Todo without an owner whose activity could not be read, so
  /// whether the dispatcher still chooses is unknown (M-m3): read again soon.
  final Set<String> ownerUnknown;

  /// The columns left to right, with the owner, creator and responsible filters applied to their cards.
  List<BoardColumnCards> get columns => [
        for (final c in board?.columns ?? const <BoardColumnCards>[])
          BoardColumnCards(
            column: c.column,
            tickets: owner == null && createdBy == null && responsible == null
                ? c.tickets
                : [
                    for (final t in c.tickets)
                      if ((owner == null || (t.ownerEmployeeId ?? noOwner) == owner) && (createdBy == null || t.createdByUser == createdBy) && (responsible == null || (t.responsibleUserId ?? noResponsible) == responsible)) t,
                  ],
          ),
      ];

  List<TicketView> get tickets => [for (final c in columns) ...c.tickets];

  BoardColumnCards? columnOf(ColumnKind kind) => columns.where((c) => c.column.kind == kind).firstOrNull;

  TicketView? ticket(String id) => tickets.where((t) => t.id == id).firstOrNull;

  BoardState copyWith({bool? loading, BoardView? board, String? error, bool? unsupported, String? projectId, bool clearProject = false, String? owner, bool clearOwner = false, String? createdBy, bool clearCreatedBy = false, String? responsible, bool clearResponsible = false, bool? needsMeOnly, ColumnKind? column, Set<String>? gaveUp, Set<String>? ownerUnknown}) => BoardState(
        loading: loading ?? this.loading,
        board: board ?? this.board,
        error: error,
        unsupported: unsupported ?? this.unsupported,
        projectId: clearProject ? null : projectId ?? this.projectId,
        owner: clearOwner ? null : owner ?? this.owner,
        createdBy: clearCreatedBy ? null : createdBy ?? this.createdBy,
        responsible: clearResponsible ? null : responsible ?? this.responsible,
        needsMeOnly: needsMeOnly ?? this.needsMeOnly,
        column: column ?? this.column,
        gaveUp: gaveUp ?? this.gaveUp,
        ownerUnknown: ownerUnknown ?? this.ownerUnknown,
      );

  @override
  List<Object?> get props => [loading, board?.asOfSeq, board?.projectId, [for (final t in board?.columns.expand((c) => c.tickets) ?? const <TicketView>[]) (t.id, t.version, t.columnId, t.responsibleUserId, t.responsibleDisabled)], [for (final c in board?.columns ?? const <BoardColumnCards>[]) (c.column.name, c.column.version, c.column.position)], error, unsupported, projectId, owner, createdBy, responsible, needsMeOnly, column, gaveUp, ownerUnknown];
}

/// The Board (MU1-05): `board.get` for the chosen project, again on every
/// change of a ticket, a task under it or the columns, so moves by others
/// show live (AC3), while the tab is on screen (else once it shows again).
/// Writes go through `ticket_actions.dart`; their events bring the new board here.
class BoardBloc extends Bloc<BoardEvent, BoardState> {
  /// [ticketTask] tells whether a task belongs to a ticket (a delegation under
  /// one); main tasks of the cards are known here.
  BoardBloc(this._repository, {String? projectId, this.timeout = const Duration(seconds: 10), this.retryAfter = const Duration(seconds: 15), this.ticketTask}) : super(BoardState(projectId: projectId)) {
    on<BoardRefreshed>(_refresh, transformer: debounceRestartable());
    on<BoardProjectChosen>((e, emit) {
      emit(state.copyWith(projectId: e.projectId, clearProject: e.projectId == null, loading: true));
      add(const BoardRefreshed());
    });
    on<BoardOwnerChosen>((e, emit) => emit(state.copyWith(owner: e.owner, clearOwner: e.owner == null)));
    on<BoardCreatorChosen>((e, emit) => emit(state.copyWith(createdBy: e.userId, clearCreatedBy: e.userId == null)));
    on<BoardResponsibleChosen>((e, emit) => emit(state.copyWith(responsible: e.userId, clearResponsible: e.userId == null)));
    on<BoardNeedsMeOnly>((e, emit) => emit(state.copyWith(needsMeOnly: e.on)));
    on<BoardColumnShown>((e, emit) => emit(state.copyWith(column: e.kind)));
    on<BoardShown>((e, emit) {
      _visible = e.visible;
      if (_visible && _stale) {
        _stale = false;
        add(const BoardRefreshed());
      }
    });
    _subs = [
      _repository.online.listen((_) => _changed()),
      _repository.boardChanges.where(_concerns).listen((_) => _changed()),
    ];
    add(const BoardRefreshed());
  }

  final AureliusRepository _repository;

  /// After this long without an answer the board shows its error (§3.3).
  final Duration timeout;

  /// After a failed read of a card's activity, the board is read again this much later.
  final Duration retryAfter;
  Timer? _retry;
  late final List<StreamSubscription<void>> _subs;

  /// Whether the dispatcher gave up, per ticket version: asked once per version.
  final _gaveUpAt = <String, (int, bool)>{};

  final bool Function(String taskId)? ticketTask;

  /// The main tasks of the cards on the last board.
  var _roots = <String>{};
  var _visible = true;

  /// Something changed while the tab was hidden.
  var _stale = false;

  /// Tasks without a ticket do not touch the board, nor does who follows a ticket.
  bool _concerns(Event e) => e.kind != EventKind.ticketFollowChanged && (!e.kind.wire.startsWith('task.') || _roots.contains(e.aggregateId) || (ticketTask?.call(e.aggregateId) ?? false));

  void _changed() {
    if (isClosed) return;
    if (_visible) {
      add(const BoardRefreshed());
    } else {
      _stale = true;
    }
  }

  Future<void> _refresh(BoardRefreshed event, Emitter<BoardState> emit) async {
    try {
      // Testing only shows when the server has it (ticket #5).
      final board = boardShown(await _repository.board(projectId: state.projectId).timeout(timeout), testing: _repository.linkState.has(boardTestingFeature));
      _roots = {for (final t in board.columns.expand((c) => c.tickets)) ?t.rootTaskId};
      final (gaveUp, unknown) = await _gaveUp(board);
      emit(state.copyWith(loading: false, board: board, unsupported: false, gaveUp: gaveUp, ownerUnknown: unknown));
      _retry?.cancel();
      if (unknown.isNotEmpty) _retry = Timer(retryAfter, _changed);
    } on TimeoutException {
      emit(state.copyWith(loading: false, error: 'The PC did not answer in time.'));
    } on DisconnectedException catch (e) {
      // Offline: the last known board stays (dimmed by the page).
      emit(state.copyWith(loading: false, error: state.board == null ? e.message : null));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, unsupported: e.code == ErrorCode.unknownMethod, error: e.shown));
    }
  }

  /// The Todo cards without an owner whose dispatcher could not choose: read
  /// from their activity, since only there the server says it (`needs: owner`).
  /// Second: the cards whose activity could not be read (not cached, so asked again).
  Future<(Set<String>, Set<String>)> _gaveUp(BoardView board) async {
    final out = <String>{};
    final unknown = <String>{};
    for (final t in board.columns.expand((c) => c.tickets)) {
      if (t.column != ColumnKind.todo || t.ownerEmployeeId != null) continue;
      final known = _gaveUpAt[t.id];
      if (known != null && known.$1 == t.version) {
        if (known.$2) out.add(t.id);
        continue;
      }
      try {
        final page = await _repository.timeline(correlationId: t.id, activity: true, limit: 20);
        final gave = dispatcherGaveUp(page.cards);
        _gaveUpAt[t.id] = (t.version, gave);
        if (gave) out.add(t.id);
      } on Object {
        unknown.add(t.id);
      }
    }
    return (out, unknown);
  }

  @override
  Future<void> close() async {
    _retry?.cancel();
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
