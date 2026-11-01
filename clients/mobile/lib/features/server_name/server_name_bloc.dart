import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../data/aurelius_repository.dart';
import '../../data/views.dart';

sealed class ServerNameEvent {
  const ServerNameEvent();
}

/// Read the server's name (Settings opened, the link came back, Try again).
class ServerNameRefreshed extends ServerNameEvent {
  const ServerNameRefreshed();
}

class _ServerNameRead extends ServerNameEvent {
  const _ServerNameRead(this.view);

  final ServerNameView view;
}

class ServerNameState extends Equatable {
  const ServerNameState({this.view, this.loading = false, this.error});

  /// What `server.get_name` said last; null before the first answer and on
  /// a server without the feature `server_name`.
  final ServerNameView? view;
  final bool loading;

  /// Why the last read failed, with a Try again.
  final String? error;

  @override
  List<Object?> get props => [view?.name, view?.chosen, view?.computerName, view?.version, loading, error];
}

/// The server's name in Settings (SN-01 AC1): the name, and the computer's
/// as the default when a board member chose one. Follows `server.renamed`
/// through the connection's reads (SN-02 AC1). A server without the feature
/// is never asked (SN-02 AC4).
class ServerNameBloc extends Bloc<ServerNameEvent, ServerNameState> {
  ServerNameBloc(this._repository) : super(const ServerNameState()) {
    on<ServerNameRefreshed>(_refresh);
    on<_ServerNameRead>((event, emit) => emit(ServerNameState(view: event.view)));
    _subs = [
      _repository.serverNameChanges.listen((view) => add(_ServerNameRead(view))),
      _repository.online.listen((_) => add(const ServerNameRefreshed())),
    ];
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<Object?>> _subs;

  Future<void> _refresh(ServerNameRefreshed event, Emitter<ServerNameState> emit) async {
    if (!_repository.linkState.has(serverNameFeature)) return emit(const ServerNameState());
    emit(ServerNameState(view: state.view, loading: true));
    try {
      emit(ServerNameState(view: await _repository.serverName()));
    } catch (e) {
      emit(ServerNameState(view: state.view, error: "Could not read the server's name. $e"));
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
