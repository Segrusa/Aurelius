import 'dart:async';

import 'package:equatable/equatable.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/bloc/transformers.dart';
import '../../core/net/connection.dart';
import '../../core/net/protocol.dart';
import '../../data/aurelius_repository.dart';
import '../../data/commands.dart';
import '../../data/views.dart';

sealed class ProjectEvent {
  const ProjectEvent();
}

class ProjectRefreshed extends ProjectEvent {
  const ProjectRefreshed();
}

/// Any project or task command from this screen.
class ProjectCommandSent extends ProjectEvent {
  const ProjectCommandSent(this.command, {this.done, this.expectedVersion, this.retry = true, this.then = const []});

  final Map<String, dynamic> command;

  /// Sent in order once [command] committed.
  final List<Map<String, dynamic>> then;

  /// Shown after it committed (e.g. "Task cancelled.").
  final String? done;
  final int? expectedVersion;

  /// Decisions (cancel, archive) are never replayed later.
  final bool retry;
}

class ProjectNoticeShown extends ProjectEvent {
  const ProjectNoticeShown();
}

enum TaskFilter { active, waiting, completed }

class ProjectState extends Equatable {
  const ProjectState({this.loading = true, this.project, this.tasks = const [], this.busy = false, this.notice, this.lastResult});

  final bool loading;
  final ProjectSummary? project;
  final List<TaskView> tasks;
  final bool busy;
  final String? notice;

  /// The last command result (e.g. `TaskDiscussed` → open the lead's chat).
  final CommandResult? lastResult;

  List<TaskView> filtered(TaskFilter filter) => switch (filter) {
        TaskFilter.active => tasks.where((t) => t.isActive).toList(),
        TaskFilter.waiting => tasks.where((t) => t.isOpen && (t.needsYou || t.waitsToStart)).toList(),
        TaskFilter.completed => tasks.where((t) => t.isDone && !t.hasLiveRun).toList(),
      };

  TaskView? task(String id) => tasks.where((t) => t.id == id).firstOrNull;

  ProjectState copyWith({bool? loading, ProjectSummary? project, List<TaskView>? tasks, bool? busy, String? notice, CommandResult? lastResult}) =>
      ProjectState(
        loading: loading ?? this.loading,
        project: project ?? this.project,
        tasks: tasks ?? this.tasks,
        busy: busy ?? this.busy,
        notice: notice,
        lastResult: lastResult,
      );

  @override
  List<Object?> get props => [loading, project, tasks, busy, notice, lastResult];
}

/// One project and its tasks, and every action on them. Employees are
/// company-wide: they come from the CompanyBloc.
class ProjectBloc extends Bloc<ProjectEvent, ProjectState> {
  ProjectBloc(this._repository, this.projectId) : super(const ProjectState()) {
    on<ProjectRefreshed>(_refresh, transformer: debounceRestartable());
    on<ProjectCommandSent>(_command);
    on<ProjectNoticeShown>((e, emit) => emit(state.copyWith()));
    _subs = [
      _repository.online.listen((_) => add(const ProjectRefreshed())),
      _repository.changes.where((e) => e.projectId == null || e.projectId == projectId).listen((_) => add(const ProjectRefreshed())),
    ];
    if (_repository.linkState.isOnline) add(const ProjectRefreshed());
  }

  final AureliusRepository _repository;
  final String projectId;
  late final List<StreamSubscription<void>> _subs;

  Future<void> _refresh(ProjectRefreshed event, Emitter<ProjectState> emit) async {
    try {
      final projects = await _repository.projects();
      final tasks = await _repository.tasks(projectId: projectId);
      tasks.sort((a, b) => b.updatedAtMs.compareTo(a.updatedAtMs));
      emit(state.copyWith(
        loading: false,
        project: projects.where((p) => p.id == projectId).firstOrNull,
        tasks: tasks,
      ));
    } on DisconnectedException {
      emit(state.copyWith(loading: false));
    } on ProtocolException catch (e) {
      emit(state.copyWith(loading: false, notice: e.message));
    }
  }

  Future<void> _command(ProjectCommandSent event, Emitter<ProjectState> emit) async {
    emit(state.copyWith(busy: true));
    try {
      final receipt = await _repository.write(event.command, expectedVersion: event.expectedVersion, retry: event.retry);
      for (final next in event.then) {
        await _repository.write(next, retry: event.retry);
      }
      emit(state.copyWith(busy: false, notice: event.done, lastResult: receipt.command));
      add(const ProjectRefreshed());
    } on NotSentException catch (e) {
      emit(state.copyWith(busy: false, notice: e.toString()));
    } on ProtocolException catch (e) {
      emit(state.copyWith(busy: false, notice: e.isConflict ? 'Someone changed this in the meantime. Showing the latest version.' : e.message));
      add(const ProjectRefreshed());
    }
  }

  void send(Map<String, dynamic> command, {String? done, int? expectedVersion, bool retry = true, List<Map<String, dynamic>> then = const []}) =>
      add(ProjectCommandSent(command, done: done, expectedVersion: expectedVersion, retry: retry, then: then));

  // Convenience for the views.
  void cancelTask(String taskId) => send(Commands.cancelTask(taskId), done: 'Task cancelled.', retry: false);
  void assignTask(String taskId, String? employeeId, String? name) =>
      send(Commands.assignTask(taskId, employeeId), done: name == null ? 'The task is a draft again.' : 'Task given to $name.', retry: false);

  @override
  Future<void> close() async {
    for (final s in _subs) {
      await s.cancel();
    }
    return super.close();
  }
}
