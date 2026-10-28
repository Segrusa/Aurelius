import 'dart:async';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class UpdatesEvent {
  const UpdatesEvent();
}

class UpdatesRequested extends UpdatesEvent {
  const UpdatesRequested({this.check = false});

  /// Ask GitHub for the latest release now (otherwise the server's last check).
  final bool check;
}

/// Download, verify and install the latest release on the server.
class UpdateInstallRequested extends UpdatesEvent {
  const UpdateInstallRequested();
}

/// Restart the server's background service without updating it.
class ServerRestartRequested extends UpdatesEvent {
  const ServerRestartRequested();
}

class _ServerBack extends UpdatesEvent {
  const _ServerBack();
}

/// `stopping`/`rebooting`: a plain restart (asked, then waiting until the
/// server is back); the others belong to installing an update.
enum UpdatePhase { idle, downloading, installing, restarting, stopping, rebooting }

class UpdatesState extends Equatable {
  const UpdatesState({this.loading = true, this.status, this.phase = UpdatePhase.idle, this.target, this.error, this.notice});

  final bool loading;
  final UpdateStatus? status;
  final UpdatePhase phase;

  /// The version being installed.
  final String? target;
  final String? error;

  /// One-off message for a snackbar.
  final String? notice;

  bool get busy => phase != UpdatePhase.idle;

  UpdatesState copyWith({bool? loading, UpdateStatus? status, UpdatePhase? phase, String? target, String? error, String? notice}) => UpdatesState(
        loading: loading ?? this.loading,
        status: status ?? this.status,
        phase: phase ?? this.phase,
        target: target ?? this.target,
        error: error,
        notice: notice,
      );

  @override
  List<Object?> get props => [loading, status, phase, target, error, notice];
}

/// The server's updates, from the phone. After installing, the server
/// restarts; the screen waits until it is back and checks the new version.
class UpdatesBloc extends Bloc<UpdatesEvent, UpdatesState> {
  UpdatesBloc(this._repository, {this.dropTimeout = const Duration(seconds: 10)}) : super(const UpdatesState()) {
    on<UpdatesRequested>(_load, transformer: droppable());
    on<UpdateInstallRequested>(_install, transformer: droppable());
    on<ServerRestartRequested>(_restart, transformer: droppable());
    on<_ServerBack>(_back);
    _online = _repository.online.listen((_) => add(const _ServerBack()));
    add(const UpdatesRequested());
  }

  final AureliusRepository _repository;

  /// How long a restarted server may take to close the connection.
  final Duration dropTimeout;
  late final StreamSubscription<void> _online;

  Future<void> _load(UpdatesRequested event, Emitter<UpdatesState> emit) async {
    try {
      final status = await _repository.updateStatus(check: event.check);
      emit(state.copyWith(loading: false, status: status, notice: event.check && !status.available && status.enabled ? 'The server is up to date.' : null));
    } catch (e) {
      emit(state.copyWith(loading: false, error: e.toString()));
    }
  }

  Future<void> _install(UpdateInstallRequested event, Emitter<UpdatesState> emit) async {
    emit(state.copyWith(phase: UpdatePhase.downloading));
    final UpdateStatus prepared;
    try {
      prepared = await _repository.prepareUpdate();
    } catch (e) {
      emit(state.copyWith(phase: UpdatePhase.idle, notice: _message(e)));
      return;
    }
    final staged = prepared.staged;
    if (staged == null || prepared.error != null) {
      emit(state.copyWith(phase: UpdatePhase.idle, status: prepared, notice: prepared.error ?? 'No newer version available.'));
      return;
    }
    emit(state.copyWith(phase: UpdatePhase.installing, status: prepared, target: staged));
    try {
      await _repository.applyUpdate();
    } on DisconnectedException {
      // The server stops to restart; the answer may not make it. Wait for it.
    } catch (e) {
      emit(state.copyWith(phase: UpdatePhase.idle, notice: _message(e)));
      return;
    }
    emit(state.copyWith(phase: UpdatePhase.restarting));
  }

  Future<void> _restart(ServerRestartRequested event, Emitter<UpdatesState> emit) async {
    if (state.busy) return;
    if (!_repository.linkState.isOnline) {
      emit(state.copyWith(notice: 'Not connected to the server.'));
      return;
    }
    emit(state.copyWith(phase: UpdatePhase.stopping));
    // The server answers, then closes the connection: watch for that drop.
    final dropped = _repository.link.firstWhere((s) => !s.isOnline).then((_) => true, onError: (_) => false);
    try {
      await _repository.restartServer();
    } on DisconnectedException {
      // Expected: the server went away.
    } on ProtocolException catch (e) {
      emit(state.copyWith(phase: UpdatePhase.idle, notice: e.message));
      return;
    } catch (e) {
      emit(state.copyWith(phase: UpdatePhase.idle, notice: _message(e)));
      return;
    }
    if (!await dropped.timeout(dropTimeout, onTimeout: () => false)) {
      emit(state.copyWith(phase: UpdatePhase.idle, notice: 'The server did not restart.'));
      return;
    }
    emit(state.copyWith(phase: UpdatePhase.rebooting));
  }

  /// Back online (after the restart, or any reconnect): read the status again.
  Future<void> _back(_ServerBack event, Emitter<UpdatesState> emit) async {
    // Still waiting for the answer (or for the connection to drop).
    if (state.phase == UpdatePhase.installing || state.phase == UpdatePhase.stopping) return;
    try {
      final status = await _repository.updateStatus();
      if (state.phase == UpdatePhase.rebooting) {
        emit(state.copyWith(phase: UpdatePhase.idle, status: status, notice: 'The server restarted.'));
      } else if (state.phase == UpdatePhase.restarting) {
        final done = state.target == null || status.current == state.target;
        emit(state.copyWith(
          phase: UpdatePhase.idle,
          status: status,
          notice: done ? 'The server now runs Aurelius ${status.current}.' : 'The server is back on ${status.current}; the update did not apply.',
        ));
      } else {
        emit(state.copyWith(loading: false, status: status));
      }
    } catch (_) {
      // The next reconnect tries again.
    }
  }

  String _message(Object e) => e is ProtocolException ? e.message : e.toString();

  @override
  Future<void> close() {
    _online.cancel();
    return super.close();
  }
}
