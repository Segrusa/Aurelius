import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/protocol/protocol.dart';
import '../../data/aurelius_repository.dart';
import 'session_rules.dart';

sealed class SessionsEvent {
  const SessionsEvent();
}

/// Read `work_session.list` again (also the Try again of a failed read).
class SessionsRefreshed extends SessionsEvent {
  const SessionsRefreshed();
}

/// Whether the screen of this bloc is on screen; hidden, changes only mark the list stale.
class SessionsShown extends SessionsEvent {
  const SessionsShown(this.visible);

  final bool visible;
}

class _FeaturesChanged extends SessionsEvent {
  const _FeaturesChanged();
}

class SessionsState extends Equatable {
  const SessionsState({this.supported = false, this.loading = true, this.sessions = const [], this.error});

  /// The server has work sessions (feature `work_sessions`).
  final bool supported;

  /// Until the first `work_session.list` answered (or failed).
  final bool loading;

  /// Newest activity first.
  final List<WorkSessionView> sessions;

  /// Why the last read failed; the sessions read before stay shown.
  final String? error;

  /// How many work right now (the header's count).
  int get working => activeSessions(sessions);

  WorkSessionView? byId(String conversationId) => sessions.where((s) => s.conversationId == conversationId).firstOrNull;

  SessionsState copyWith({bool? supported, bool? loading, List<WorkSessionView>? sessions, String? error}) =>
      SessionsState(supported: supported ?? this.supported, loading: loading ?? this.loading, sessions: sessions ?? this.sessions, error: error);

  @override
  List<Object?> get props => [
        supported,
        loading,
        [for (final s in sessions) (s.conversationId, s.lastActivityMs, s.running, s.dispatch, s.employeeId, s.employeeName, s.title)],
        error,
      ];
}

/// The work sessions of the reader (`work_session.list`). It reads again on
/// every chat change while its screen is on screen (TickerMode); hidden, it
/// marks itself stale and reads once it shows again. Without the server
/// feature it reads nothing.
class SessionsBloc extends Bloc<SessionsEvent, SessionsState> {
  SessionsBloc(this._repository, {this.timeout = const Duration(seconds: 10), Duration debounce = const Duration(milliseconds: 150)})
      : super(SessionsState(supported: _repository.linkState.has(workSessionsFeature), loading: _repository.linkState.has(workSessionsFeature))) {
    on<SessionsRefreshed>(_refresh, transformer: debounceRestartable(debounce));
    on<SessionsShown>((e, emit) {
      _visible = e.visible;
      if (_visible && _stale) {
        _stale = false;
        add(const SessionsRefreshed());
      }
    });
    on<_FeaturesChanged>((e, emit) {
      final supported = _repository.linkState.has(workSessionsFeature);
      if (supported == state.supported) return;
      emit(SessionsState(supported: supported, loading: supported));
      if (supported) add(const SessionsRefreshed());
    });
    _subs = [
      _repository.link.listen((_) => add(const _FeaturesChanged())),
      _repository.online.listen((_) => _changed()),
      _repository.chatChanges.listen((_) => _changed()),
    ];
    if (state.supported) add(const SessionsRefreshed());
  }

  final AureliusRepository _repository;

  /// After this long without an answer the list shows its error.
  final Duration timeout;
  late final List<StreamSubscription<void>> _subs;
  var _visible = true;
  var _stale = false;

  void _changed() {
    if (isClosed || !state.supported) return;
    if (_visible) {
      add(const SessionsRefreshed());
    } else {
      _stale = true;
    }
  }

  Future<void> _refresh(SessionsRefreshed event, Emitter<SessionsState> emit) async {
    if (!state.supported) return;
    if (state.error != null) emit(state.copyWith(loading: true));
    try {
      final sessions = await _repository.workSessions().timeout(timeout);
      emit(state.copyWith(loading: false, sessions: sessionsByActivity(sessions)));
    } on TimeoutException {
      emit(state.copyWith(loading: false, error: 'The PC did not answer in time.'));
    } on DisconnectedException catch (e) {
      emit(state.copyWith(loading: false, error: e.message));
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
