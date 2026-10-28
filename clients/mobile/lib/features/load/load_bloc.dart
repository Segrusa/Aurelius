import 'dart:async';

import 'package:bloc_concurrency/bloc_concurrency.dart';
import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class LoadEvent {
  const LoadEvent();
}

class LoadRefreshed extends LoadEvent {
  const LoadRefreshed();
}

class _LoadChanged extends LoadEvent {
  const _LoadChanged(this.snapshot);

  final LoadSnapshot snapshot;
}

class LoadNoticeShown extends LoadEvent {
  const LoadNoticeShown();
}

class LoadState extends Equatable {
  const LoadState({this.snapshot, this.loading = true, this.error, this.notice});

  final LoadSnapshot? snapshot;
  final bool loading;

  /// Why the load could not be read.
  final String? error;

  /// One-off message.
  final String? notice;

  LoadState copyWith({LoadSnapshot? snapshot, bool? loading, String? error, String? notice}) =>
      LoadState(snapshot: snapshot ?? this.snapshot, loading: loading ?? this.loading, error: error, notice: notice);

  @override
  List<Object?> get props => [snapshot, loading, error, notice];
}

/// The server's load, kept live by `load_changed`.
class LoadBloc extends Bloc<LoadEvent, LoadState> {
  LoadBloc(this._repository) : super(const LoadState()) {
    on<LoadRefreshed>(_refresh, transformer: restartable());
    on<_LoadChanged>((e, emit) => emit(state.copyWith(snapshot: e.snapshot, loading: false)));
    on<LoadNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.loadChanges.listen((s) => add(_LoadChanged(s))),
      _repository.online.listen((_) => add(const LoadRefreshed())),
    ];
    add(const LoadRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<Object?>> _subs;

  Future<void> _refresh(LoadRefreshed event, Emitter<LoadState> emit) async {
    try {
      emit(state.copyWith(snapshot: await _repository.load(), loading: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, error: e.message));
    } on DisconnectedException {
      emit(state.copyWith(loading: false));
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
