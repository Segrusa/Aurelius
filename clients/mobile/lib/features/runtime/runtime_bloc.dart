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

class RuntimeState extends Equatable {
  const RuntimeState({this.loading = true, this.snapshot, this.notice});

  final bool loading;
  final RunConfig? snapshot;
  final String? notice;

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

  RuntimeState copyWith({bool? loading, RunConfig? snapshot, String? notice}) =>
      RuntimeState(loading: loading ?? this.loading, snapshot: snapshot ?? this.snapshot, notice: notice);

  /// Model and effort for one employee's runs, with where each comes from.
  EmployeeRunView? employeeRun(String employeeId, {String? projectId, Purpose purpose = Purpose.employee}) =>
      snapshot?.employeeRun(employeeId, projectId: projectId, purpose: purpose);

  @override
  List<Object?> get props => [loading, snapshot?.defaults.values.toList(), snapshot?.scoped, snapshot?.connections, notice];
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
    _subs = [
      _repository.online.listen((_) => add(const RuntimeRefreshed())),
      _repository.companyChanges.where((e) => e.isRuntimeConfig).listen((_) => add(const RuntimeRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const RuntimeRefreshed());
  }

  final AureliusRepository _repository;
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(RuntimeRefreshed event, Emitter<RuntimeState> emit) async {
    try {
      emit(state.copyWith(loading: false, snapshot: RunConfig.of(await _repository.connections())));
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

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
