import 'dart:async';

import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/protocol.dart';
import '../../data/activity.dart';
import '../../data/aurelius_repository.dart';
import '../../data/protocol/protocol.dart';

/// Read the overview again.
class ActivityRefreshed {
  const ActivityRefreshed();
}

/// Keeps `activity.overview` current (ticket #50): on the events of runs, tasks,
/// tickets and employees, when back online, and every [every] besides. The state
/// is null on a server without the feature or before the first answer.
class ActivityBloc extends Bloc<ActivityRefreshed, ActivityOverview?> {
  ActivityBloc(this._repository, {this.every = const Duration(seconds: 30)}) : super(null) {
    on<ActivityRefreshed>(_refresh, transformer: debounceRestartable());
    _subs = [
      _repository.online.listen((_) => add(const ActivityRefreshed())),
      _repository.events.where(_concerns).listen((_) => add(const ActivityRefreshed())),
    ];
    _timer = Timer.periodic(every, (_) {
      if (!isClosed) add(const ActivityRefreshed());
    });
    add(const ActivityRefreshed());
  }

  final AureliusRepository _repository;

  /// The refresh besides what events cause.
  final Duration every;
  late final List<StreamSubscription<void>> _subs;
  late final Timer _timer;

  static bool _concerns(Event e) {
    final family = e.kind.wire.split('.').first;
    return const {'run', 'task', 'ticket', 'employee', 'approval'}.contains(family);
  }

  Future<void> _refresh(ActivityRefreshed event, Emitter<ActivityOverview?> emit) async {
    if (!_repository.linkState.has(activityOverviewFeature)) {
      emit(null);
      return;
    }
    try {
      emit(await _repository.overview());
    } on DisconnectedException {
      // Offline: the last known overview stays.
    } on ProtocolException {
      // The next event or tick asks again.
    } on TimeoutException {
      // The next event or tick asks again.
    }
  }

  @override
  Future<void> close() async {
    _timer.cancel();
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
