import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';

/// The server keeps a model, effort and mode per board member.
const userRunDefaultsFeature = 'user_run_defaults';

/// One field of My defaults.
enum MyDefaultsField { model, effort, mode, subagentModel }

/// [defaults] with [field] set to [value]; null clears it (Default).
UserRunDefaults withMyDefault(UserRunDefaults defaults, MyDefaultsField field, String? value) => UserRunDefaults(
      model: field == MyDefaultsField.model ? value : defaults.model,
      effort: field == MyDefaultsField.effort ? value : defaults.effort,
      mode: field == MyDefaultsField.mode ? (value == null ? null : AgentMode.fromJson(value)) : defaults.mode,
      subagentModel: field == MyDefaultsField.subagentModel ? value : defaults.subagentModel,
    );

sealed class MyDefaultsEvent {
  const MyDefaultsEvent();
}

/// Read the defaults and the models on offer (the page opened, Try again).
class MyDefaultsRefreshed extends MyDefaultsEvent {
  const MyDefaultsRefreshed();
}

/// The board member chose a value; null is Default.
class MyDefaultChosen extends MyDefaultsEvent {
  const MyDefaultChosen(this.field, this.value);

  final MyDefaultsField field;
  final String? value;
}

class MyDefaultsState extends Equatable {
  const MyDefaultsState({this.view, this.models = const [], this.error, this.saving = false});

  /// What the server said last; null before the first answer.
  final UserRunDefaultsView? view;

  /// The models the connections offer.
  final List<String> models;

  /// Why the last read or change failed.
  final String? error;

  /// A change is on its way.
  final bool saving;

  MyDefaultsState copyWith({UserRunDefaultsView? view, List<String>? models, String? error, bool? saving}) =>
      MyDefaultsState(view: view ?? this.view, models: models ?? this.models, error: error, saving: saving ?? this.saving);

  @override
  List<Object?> get props => [view, models, error, saving];
}

/// The signed-in board member's own run defaults.
class MyDefaultsBloc extends Bloc<MyDefaultsEvent, MyDefaultsState> {
  MyDefaultsBloc(this._repository) : super(const MyDefaultsState()) {
    on<MyDefaultsRefreshed>(_refresh);
    on<MyDefaultChosen>(_choose);
  }

  final AureliusRepository _repository;

  Future<void> _refresh(MyDefaultsRefreshed event, Emitter<MyDefaultsState> emit) async {
    try {
      // Both at once: neither waits for the other.
      final (view, connections) = await (_repository.myRunDefaults(), _repository.connections()).wait;
      final models = {for (final c in connections.connections) ...c.models}.toList();
      emit(MyDefaultsState(view: view, models: models));
    } catch (e) {
      emit(state.copyWith(error: e.toString(), saving: false));
    }
  }

  Future<void> _choose(MyDefaultChosen event, Emitter<MyDefaultsState> emit) async {
    final view = state.view;
    if (view == null || state.saving) return;
    emit(state.copyWith(saving: true));
    try {
      await _repository.setMyRunDefaults(withMyDefault(view.defaults, event.field, event.value));
      emit(state.copyWith(view: await _repository.myRunDefaults(), saving: false));
    } catch (e) {
      emit(state.copyWith(error: e.toString(), saving: false));
    }
  }
}
