import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/views.dart';
import '../../data/run_config.dart';
import '../my_defaults/my_defaults_bloc.dart' show userRunDefaultsFeature;
import '../my_defaults/run_default_label.dart' show runDefaultsOfFeature;

sealed class RuntimeEvent {
  const RuntimeEvent();
}

class RuntimeRefreshed extends RuntimeEvent {
  const RuntimeRefreshed();
}

/// `null` = inherit: the next scope up, or the connection's default model.
class RuntimeModelSelected extends RuntimeEvent {
  const RuntimeModelSelected(this.purpose, this.model, {this.scope = RunScope.global});

  final Purpose purpose;
  final String? model;
  final RunScope scope;
}

/// `null` = inherit: the next scope up, or the runtime's default effort.
class RuntimeEffortSelected extends RuntimeEvent {
  const RuntimeEffortSelected(this.purpose, this.effort, {this.scope = RunScope.global});

  final Purpose purpose;
  final String? effort;
  final RunScope scope;
}

class RuntimeNoticeShown extends RuntimeEvent {
  const RuntimeNoticeShown();
}

/// A screen shows work on [userId]'s login: read their run defaults once, for
/// "Kim's default · …" (feature `run_defaults_of`).
class RuntimeLoginWanted extends RuntimeEvent {
  const RuntimeLoginWanted(this.userId);

  final String userId;
}

class RuntimeState extends Equatable {
  const RuntimeState({this.loading = true, this.snapshot, this.notice, this.mine, this.others = const {}});

  final bool loading;
  final RunConfig? snapshot;
  final String? notice;

  /// The signed-in board member's own run defaults (feature `user_run_defaults`);
  /// null on an older server or before the first read.
  final UserRunDefaultsView? mine;

  /// Other board members' run defaults by user id, read when a screen shows
  /// work on their login.
  final Map<String, UserRunDefaultsView> others;

  /// The defaults a run on [loginId]'s login uses: the reader's own when it is
  /// theirs (or unknown), else the other board member's once read.
  UserRunDefaultsView? defaultsOf(String? loginId, String? meId) =>
      loginId == null || meId == null || loginId == meId ? mine : others[loginId];

  /// The connection, models and effort the next run of [purpose] uses
  /// (global default, as the desktop's model popup). `null` without a connection.
  RunChoice? choice(Purpose purpose) {
    final snapshot = this.snapshot;
    final defaults = snapshot?.defaults[purpose];
    final id = defaults?.connectionId;
    if (snapshot == null || defaults == null || id == null) return null;
    final connection = snapshot.connections.where((c) => c.id == id).firstOrNull;
    return connection == null ? null : RunChoice(defaults: defaults, connection: connection);
  }

  RuntimeState copyWith({bool? loading, RunConfig? snapshot, String? notice, UserRunDefaultsView? mine, Map<String, UserRunDefaultsView>? others}) =>
      RuntimeState(
        loading: loading ?? this.loading,
        snapshot: snapshot ?? this.snapshot,
        notice: notice,
        mine: mine ?? this.mine,
        others: others ?? this.others,
      );

  /// Model and effort for one employee's runs, with where each comes from.
  EmployeeRunView? employeeRun(String employeeId, {String? projectId, Purpose purpose = Purpose.employee}) =>
      snapshot?.employeeRun(employeeId, projectId: projectId, purpose: purpose);

  @override
  List<Object?> get props => [loading, snapshot?.defaults.values.toList(), snapshot?.scoped, snapshot?.connections, notice, mine, others];
}

/// Model and effort for chat and employee runs: the same global defaults the
/// desktop's model popup (Ctrl+M) edits.
class RuntimeBloc extends Bloc<RuntimeEvent, RuntimeState> {
  RuntimeBloc(this._repository) : super(const RuntimeState()) {
    on<RuntimeRefreshed>(_refresh, transformer: debounceRestartable());
    on<RuntimeModelSelected>((e, emit) {
      final model = e.model;
      final label = model != null ? ModelCatalog.friendly(model) : (e.scope == RunScope.global ? 'Default' : 'Inherit');
      return _save(emit, e.purpose, e.scope, (d) => d.withModel(model), 'Model: $label');
    });
    on<RuntimeEffortSelected>((e, emit) {
      final label = e.effort == null && e.scope != RunScope.global ? 'Inherit' : ModelCatalog.effortLabel(e.effort);
      return _save(emit, e.purpose, e.scope, (d) => d.withEffort(e.effort), 'Effort: $label');
    });
    on<RuntimeNoticeShown>((e, emit) => emit(state.copyWith()));
    on<RuntimeLoginWanted>(_wantLogin);
    _subs = [
      _repository.online.listen((_) => add(const RuntimeRefreshed())),
      // A board member's own defaults change what "Your default" comes to.
      _repository.companyChanges
          .where((e) => e.isRuntimeConfig || e.kind == EventKind.userRunDefaultsChanged)
          .listen((_) => add(const RuntimeRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const RuntimeRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(RuntimeRefreshed event, Emitter<RuntimeState> emit) async {
    try {
      // Both at once; a failed read of the defaults keeps the last ones (the labels lose only the value).
      final mine = _repository.linkState.has(userRunDefaultsFeature)
          ? _repository.myRunDefaults().then<UserRunDefaultsView?>((v) => v, onError: (Object _) => state.mine)
          : Future<UserRunDefaultsView?>.value();
      final connections = await _repository.connections();
      // Another board member's defaults may have changed too: they are read again when shown.
      emit(RuntimeState(loading: false, snapshot: RunConfig.of(connections), mine: await mine));
    } on DisconnectedException {
      emit(state.copyWith(loading: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, notice: e.message));
    }
  }

  Future<void> _save(Emitter<RuntimeState> emit, Purpose purpose, RunScope scope, RunDefaults Function(RunDefaults) change, String done) async {
    final before = state.snapshot;
    // An employee or project without its own layer yet starts all inherit, as do
    // subagent runs (they follow the employee's runs); employee runs need the
    // PC to have chosen their connection first.
    final needsConnection = scope == RunScope.global && purpose == Purpose.employee;
    final current = before?.at(scope, purpose) ?? (needsConnection ? null : RunDefaults(purpose: purpose, scope: scope, settings: const RunSettings()));
    if (before == null || current == null) {
      emit(state.copyWith(notice: 'Choose a connection in Aurelius on the PC first.'));
      return;
    }
    final next = change(current);
    // Optimistic, like the desktop; the daemon confirms with a fresh snapshot.
    emit(state.copyWith(snapshot: before.replacing(next)));
    try {
      await _repository.write(Commands.setRunDefaults(purpose, next.settings, scope: scope), retry: false);
      emit(state.copyWith(notice: done));
    } on NotSentException catch (e) {
      emit(state.copyWith(snapshot: before, notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(snapshot: before, notice: e.message));
    }
  }

  Future<void> _wantLogin(RuntimeLoginWanted event, Emitter<RuntimeState> emit) async {
    if (!_repository.linkState.has(runDefaultsOfFeature) || state.others.containsKey(event.userId)) return;
    try {
      final view = await _repository.runDefaultsOf(event.userId);
      emit(state.copyWith(others: {...state.others, event.userId: view}));
    } on DisconnectedException {
      // Read again when the screen shows it after the reconnect.
    } on ProtocolException {
      // The label stays "Kim's default" without a value.
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
