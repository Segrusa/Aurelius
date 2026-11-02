import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

/// The four lists of My tickets (US-A4, ui §5.3).
enum MyTab {
  responsible('Responsible', 'Responsible'),
  created('Created by me', 'Created'),
  following('Following', 'Following'),
  waiting('Waiting on me', 'Waiting');

  const MyTab(this.label, this.short);

  /// The full label, also for the screen reader.
  final String label;

  /// What a chip shows at a large text size.
  final String short;
}

sealed class MyTicketsEvent {
  const MyTicketsEvent();
}

class MyTicketsRefreshed extends MyTicketsEvent {
  const MyTicketsRefreshed();
}

/// Whether the Board tab is on screen; hidden, changes only mark the lists stale.
class MyTicketsShown extends MyTicketsEvent {
  const MyTicketsShown(this.visible);

  final bool visible;
}

class MyTicketsProjectChosen extends MyTicketsEvent {
  const MyTicketsProjectChosen(this.projectId);

  final String? projectId;
}

class MyTicketsDoneShown extends MyTicketsEvent {
  const MyTicketsDoneShown(this.on);

  final bool on;
}

class MyTicketsState extends Equatable {
  const MyTicketsState({this.loading = true, this.mine, this.error, this.projectId, this.includeDone = false});

  /// Until the first `ticket.my_tickets` answered.
  final bool loading;
  final MyTicketsView? mine;

  /// Why they could not be read; the last known lists stay shown.
  final String? error;
  final String? projectId;
  final bool includeDone;

  List<TicketView> get tickets => mine?.tickets ?? const [];

  /// The tickets of [tab] for [me]. [needsMe] is the ◆ of the card (B6): Waiting on me is exactly that.
  List<TicketView> inTab(MyTab tab, {required String? me, required bool Function(TicketView) needsMe}) => [
        for (final t in tickets)
          if (switch (tab) {
            MyTab.responsible => me != null && t.responsibleUserId == me,
            MyTab.created => me != null && t.createdByUser == me,
            MyTab.following => mine?.following.contains(t.id) ?? false,
            MyTab.waiting => needsMe(t),
          })
            t,
      ];

  MyTicketsState copyWith({bool? loading, MyTicketsView? mine, String? error, String? projectId, bool clearProject = false, bool? includeDone}) => MyTicketsState(
        loading: loading ?? this.loading,
        mine: mine ?? this.mine,
        error: error,
        projectId: clearProject ? null : projectId ?? this.projectId,
        includeDone: includeDone ?? this.includeDone,
      );

  @override
  List<Object?> get props => [loading, [for (final t in tickets) (t.id, t.version, t.columnId, t.responsibleUserId, t.responsibleDisabled)], mine?.following, error, projectId, includeDone];
}

/// My tickets (A4): `ticket.my_tickets` for the chosen project, again on every
/// change of a ticket (or who follows one) while the Board tab is on screen.
class MyTicketsBloc extends Bloc<MyTicketsEvent, MyTicketsState> {
  MyTicketsBloc(this._repository, {this.timeout = const Duration(seconds: 10)}) : super(const MyTicketsState()) {
    on<MyTicketsRefreshed>(_refresh, transformer: debounceRestartable());
    on<MyTicketsProjectChosen>((e, emit) {
      emit(state.copyWith(projectId: e.projectId, clearProject: e.projectId == null, loading: true));
      add(const MyTicketsRefreshed());
    });
    on<MyTicketsDoneShown>((e, emit) {
      emit(state.copyWith(includeDone: e.on, loading: true));
      add(const MyTicketsRefreshed());
    });
    on<MyTicketsShown>((e, emit) {
      _visible = e.visible;
      if (_visible && _stale) {
        _stale = false;
        add(const MyTicketsRefreshed());
      }
    });
    _subs = [
      _repository.online.listen((_) => _changed()),
      _repository.boardChanges.where(_concerns).listen((_) => _changed()),
    ];
    add(const MyTicketsRefreshed());
  }

  final AureliusRepository _repository;

  /// After this long without an answer the lists show their error.
  final Duration timeout;
  late final List<StreamSubscription<void>> _subs;

  /// The main tasks of the tickets on the last read.
  var _roots = <String>{};
  var _visible = true;
  var _stale = false;

  bool _concerns(Event e) => !e.kind.wire.startsWith('task.') || _roots.contains(e.aggregateId);

  void _changed() {
    if (isClosed) return;
    if (_visible) {
      add(const MyTicketsRefreshed());
    } else {
      _stale = true;
    }
  }

  Future<void> _refresh(MyTicketsRefreshed event, Emitter<MyTicketsState> emit) async {
    try {
      final mine = await _repository.myTickets(projectId: state.projectId, includeDone: state.includeDone).timeout(timeout);
      _roots = {for (final t in mine.tickets) ?t.rootTaskId};
      emit(state.copyWith(loading: false, mine: mine));
    } on TimeoutException {
      emit(state.copyWith(loading: false, error: 'The PC did not answer in time.'));
    } on DisconnectedException catch (e) {
      // Offline: the last known lists stay (dimmed by the page).
      emit(state.copyWith(loading: false, error: state.mine == null ? e.message : null));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, error: e.shown));
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
