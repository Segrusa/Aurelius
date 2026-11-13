import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../data/aurelius_repository.dart';
import '../../data/protocol/models.dart';

sealed class EmployeeRunsEvent {
  const EmployeeRunsEvent();
}

/// Read the overview (the page opened, Try again).
class EmployeeRunsRefreshed extends EmployeeRunsEvent {
  const EmployeeRunsRefreshed();
}

class EmployeeRunsState extends Equatable {
  const EmployeeRunsState({this.view, this.error});

  /// What the server said last; null before the first answer.
  final EmployeeRunsView? view;

  /// Why the last read failed, with a Try again.
  final String? error;

  @override
  List<Object?> get props => [view, error];
}

/// The model and effort of every employee, read-only on the phone.
class EmployeeRunsBloc extends Bloc<EmployeeRunsEvent, EmployeeRunsState> {
  EmployeeRunsBloc(this._repository) : super(const EmployeeRunsState()) {
    on<EmployeeRunsRefreshed>(_refresh);
  }

  final AureliusRepository _repository;

  Future<void> _refresh(EmployeeRunsRefreshed event, Emitter<EmployeeRunsState> emit) async {
    try {
      emit(EmployeeRunsState(view: await _repository.employeeRuns()));
    } catch (e) {
      emit(EmployeeRunsState(view: state.view, error: e.toString()));
    }
  }
}
