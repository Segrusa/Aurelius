import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/net/connection.dart';
import '../../data/aurelius_repository.dart';

sealed class ConnectionEvent {
  const ConnectionEvent();
}

class _LinkChanged extends ConnectionEvent {
  const _LinkChanged(this.state);

  final LinkState state;
}

/// "Retry now", or the app came back to the foreground.
class ConnectionRetryRequested extends ConnectionEvent {
  const ConnectionRetryRequested();
}

/// The honest state of the link to the active PC (spec R07: an offline host
/// shows as offline; no invented progress).
class ConnectionBloc extends Bloc<ConnectionEvent, LinkState> {
  ConnectionBloc(this._repository) : super(_repository.linkState) {
    on<_LinkChanged>((event, emit) => emit(event.state));
    on<ConnectionRetryRequested>((event, emit) => _repository.connection.wake());
    _sub = _repository.link.listen((s) => add(_LinkChanged(s)));
  }

  final AureliusRepository _repository;
  late final StreamSubscription<LinkState> _sub;

  @override
  Future<void> close() async {
    await _sub.cancel();
    return super.close();
  }
}
