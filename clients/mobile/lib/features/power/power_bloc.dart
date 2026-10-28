import 'dart:async';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class PowerEvent {
  const PowerEvent();
}

class PowerRefreshed extends PowerEvent {
  const PowerRefreshed();
}

class _PowerChanged extends PowerEvent {
  const _PowerChanged(this.power);

  final PowerState power;
}

class PcShutdownRequested extends PowerEvent {
  const PcShutdownRequested(this.when);

  final ShutdownWhen when;
}

class PcShutdownCancelled extends PowerEvent {
  const PcShutdownCancelled();
}

class PowerNoticeShown extends PowerEvent {
  const PowerNoticeShown();
}

class PowerBlocState extends Equatable {
  const PowerBlocState({this.power = const PowerState(), this.busy = false, this.notice});

  final PowerState power;
  final bool busy;

  /// One-off message (done, or why not).
  final String? notice;

  PendingShutdown? get pending => power.pending;

  PowerBlocState copyWith({PowerState? power, bool? busy, String? notice}) =>
      PowerBlocState(power: power ?? this.power, busy: busy ?? this.busy, notice: notice);

  @override
  List<Object?> get props => [power, busy, notice];
}

/// Whether the server's PC is about to shut down, kept live by `power.changed`
/// (1.12). Needs the Owner right: without it (or on an older server) nothing
/// is read and the state stays empty.
class PowerBloc extends Bloc<PowerEvent, PowerBlocState> {
  PowerBloc(this._repository) : super(const PowerBlocState()) {
    on<PowerRefreshed>(_refresh, transformer: restartable());
    on<_PowerChanged>((e, emit) => emit(state.copyWith(power: e.power)));
    on<PcShutdownRequested>(_shutdown, transformer: droppable());
    on<PcShutdownCancelled>(_cancel, transformer: droppable());
    on<PowerNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.powerChanges.listen((p) => add(_PowerChanged(p))),
      _repository.online.listen((_) => add(const PowerRefreshed())),
    ];
    add(const PowerRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<Object?>> _subs;

  /// Why a pending shutdown cannot be seen or cancelled on this link; null when it can.
  static String? unavailable(LinkState link) {
    if (!link.can('owner')) return powerNeedsOwner;
    return null;
  }

  Future<void> _refresh(PowerRefreshed event, Emitter<PowerBlocState> emit) async {
    if (unavailable(_repository.linkState) != null) return emit(state.copyWith(power: const PowerState()));
    try {
      emit(state.copyWith(power: await _repository.power()));
    } on ProtocolException {
      // Not allowed or not known: show no banner.
    } on DisconnectedException {
      // The next reconnect reads it again.
    }
  }

  Future<void> _shutdown(PcShutdownRequested event, Emitter<PowerBlocState> emit) async {
    final why = unavailable(_repository.linkState);
    if (why != null) return emit(state.copyWith(notice: why));
    emit(state.copyWith(busy: true));
    try {
      emit(state.copyWith(power: await _repository.shutdownPc(event.when), busy: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(busy: false, notice: _problem(e)));
    } on DisconnectedException catch (e) {
      // `now` may well have taken the server down before it answered.
      emit(state.copyWith(busy: false, notice: e.message));
    }
  }

  Future<void> _cancel(PcShutdownCancelled event, Emitter<PowerBlocState> emit) async {
    final why = unavailable(_repository.linkState);
    if (why != null) return emit(state.copyWith(notice: why));
    emit(state.copyWith(busy: true));
    try {
      emit(state.copyWith(power: await _repository.cancelShutdown(), busy: false, notice: 'Shutdown cancelled.'));
    } on ProtocolException catch (e) {
      emit(state.copyWith(busy: false, notice: _problem(e)));
    } on DisconnectedException catch (e) {
      emit(state.copyWith(busy: false, notice: e.message));
    }
  }

  String _problem(ProtocolException e) => e.isForbidden ? powerNeedsOwner : e.message;

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
